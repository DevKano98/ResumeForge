use resumeforge::config::Config;
use resumeforge::db;
use resumeforge::routes;
use resumeforge::services;
use resumeforge::state::{AppState, CachedAgyProbe};
use std::net::SocketAddr;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing subscriber
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "resumeforge=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Executable lookup order: tools/bin next to the app, then PATH
    if let Ok(current_path) = std::env::var("PATH") {
        let mut paths = std::env::split_paths(&current_path).collect::<Vec<_>>();
        let tools_bin = std::path::PathBuf::from("tools").join("bin");
        if tools_bin.exists() && !paths.contains(&tools_bin) {
            paths.insert(0, tools_bin);
        }
        if let Ok(new_path) = std::env::join_paths(paths) {
            std::env::set_var("PATH", new_path);
        }
    }

    let config = Config::default();
    let addr = SocketAddr::from(([127, 0, 0, 1], config.port));

    // Ensure data directory and starter templates exist
    if !config.data_dir.exists() {
        std::fs::create_dir_all(&config.data_dir)?;
    }
    let starter_dir = config.data_dir.join("templates").join("starter");
    if !starter_dir.exists() {
        let _ = std::fs::create_dir_all(&starter_dir);
    }
    let classic_dest = starter_dir.join("classic.tex");
    if !classic_dest.exists() {
        let _ = std::fs::write(&classic_dest, include_str!("../data/templates/starter/classic.tex"));
    }
    let modern_dest = starter_dir.join("modern.tex");
    if !modern_dest.exists() {
        let _ = std::fs::write(&modern_dest, include_str!("../data/templates/starter/modern.tex"));
    }

    // Initialize database pool with foreign keys and run migrations
    let db_path = config.data_dir.join("resumeforge.db");
    let pool = db::init_db(&db_path).await?;
    tracing::info!(
        "Database initialized and migrations applied at {:?}",
        db_path
    );

    let state = AppState::new(config, pool);

    // Master template integrity tripwire (Section 12, Step 5 requirement 3)
    let master_file = state.config.data_dir.join("master").join("resume.tex");
    if master_file.exists() {
        if let Ok(content) = tokio::fs::read_to_string(&master_file).await {
            use sha2::{Digest, Sha256};
            let disk_hash = format!("{:x}", Sha256::digest(content.as_bytes()));
            match db::templates::get_current_master_template(&state.db).await {
                Ok(Some(active_row)) => {
                    if active_row.content_hash != disk_hash {
                        tracing::warn!(
                            "INTEGRITY WARNING: Active master_templates row hash ({}) differs from disk hash ({}) of {:?}! A crash or manual edit may have occurred between file write and DB commit.",
                            active_row.content_hash,
                            disk_hash,
                            master_file
                        );
                    } else {
                        let hash_short = if disk_hash.len() >= 8 {
                            &disk_hash[..8]
                        } else {
                            &disk_hash
                        };
                        tracing::info!(
                            "Master template verified: active DB row matches {:?} (hash: {})",
                            master_file,
                            hash_short
                        );
                    }
                }
                Ok(None) => {
                    tracing::warn!(
                        "INTEGRITY WARNING: Master resume file exists at {:?} but no active row was found in master_templates!",
                        master_file
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to query master_templates for startup integrity check: {}",
                        e
                    );
                }
            }
        }
    }

    // Initial boot capability probe for Antigravity (Section 6, requirement 6; Phase 3, requirement 8)
    // Runs in the background so the server binds and listens immediately.
    let agy_state = state.clone();
    tokio::spawn(async move {
        tracing::info!("Running initial Antigravity capability probe in background...");
        let initial_probe =
            services::pty_runner::run_agy_prompt("reply with the single word OK", 30).await;
        let checked_at = chrono::Utc::now();
        match initial_probe {
            Ok(res) => {
                let details = match &res.status {
                    services::pty_runner::AgyOutcome::Success => {
                        format!(
                            "Responded cleanly with '{}' in {}ms",
                            res.response.trim(),
                            res.elapsed_ms
                        )
                    }
                    services::pty_runner::AgyOutcome::AiEmptyResponse => {
                        "Process exited 0 but produced zero response bytes".to_string()
                    }
                    services::pty_runner::AgyOutcome::AiToolDenied => format!(
                        "Unauthorized tool access denied: {}",
                        res.denied_actions.join(", ")
                    ),
                    services::pty_runner::AgyOutcome::AiTimeout => {
                        format!("Timed out after {}ms", res.elapsed_ms)
                    }
                    services::pty_runner::AgyOutcome::AiHung => {
                        format!("Hung and terminated after {}ms", res.elapsed_ms)
                    }
                    services::pty_runner::AgyOutcome::AiInvalidJson => {
                        "Produced invalid JSON".to_string()
                    }
                    services::pty_runner::AgyOutcome::AiError => res
                        .error_detail
                        .clone()
                        .unwrap_or_else(|| "Process exited with error".to_string()),
                };

                let cached = CachedAgyProbe {
                    outcome: res.status,
                    response: res.response,
                    elapsed_ms: res.elapsed_ms,
                    denied_actions: res.denied_actions,
                    details,
                    checked_at,
                };

                let mut cache = agy_state.agy_cache.write().await;
                *cache = Some(cached);
                tracing::info!("Initial Antigravity probe cached successfully");
            }
            Err(e) => {
                tracing::warn!("Failed initial Antigravity probe: {}", e);
            }
        }
    });

    let app = routes::create_router(state);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    let local_addr = listener.local_addr()?;
    tracing::info!("ResumeForge server listening on http://{}", local_addr);

    let shutdown_signal = async {
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!("Received shutdown signal, stopping ResumeForge...");
    };

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal)
        .await?;

    Ok(())
}
