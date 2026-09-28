use resumeforge::config::Config;
use resumeforge::db;
use resumeforge::models::template::SaveTemplateRequest;
use resumeforge::routes::templates::save_master_template;
use resumeforge::state::AppState;
use axum::extract::State;
use axum::Json;
use sha2::{Digest, Sha256};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== Step 4: State Hygiene Drill ===");
    let config = Config::default();
    let master_file = config.data_dir.join("master").join("resume.tex");

    // 1. Delete data/master/resume.tex if it exists
    if master_file.exists() {
        println!("Deleting existing hand-copied master at {:?}...", master_file);
        tokio::fs::remove_file(&master_file).await?;
        println!("Deleted successfully.");
    } else {
        println!("No master file existed at {:?}", master_file);
    }

    // 2. Initialize DB & State
    let db_path = config.data_dir.join("resumeforge.db");
    let pool = db::init_db(&db_path).await?;
    let state = AppState::new(config.clone(), pool);

    // 3. Re-save master via POST /api/template handler
    let starter_classic = include_str!("../../data/templates/starter/classic.tex");
    println!("Calling save_master_template (POST /api/template logic)...");
    let (status_code, Json(save_resp)) = save_master_template(
        State(state.clone()),
        Json(SaveTemplateRequest {
            content: starter_classic.to_string(),
            is_starter_template: true,
            adapted_from_paste: false,
        }),
    )
    .await
    .map_err(|e| anyhow::anyhow!("save_master_template failed: {:?}", e))?;

    println!("POST /api/template result: HTTP {}", status_code);
    println!("Saved master row ID: {}", save_resp.id);
    println!("Saved content_hash: {}", save_resp.content_hash);
    println!("Saved file_path: {}", save_resp.file_path);

    // 4. Verify startup tripwire logic from main.rs
    println!("\n--- Verifying main.rs startup tripwire ---");
    assert!(master_file.exists(), "Master file must exist after saving");
    let content = tokio::fs::read_to_string(&master_file).await?;
    let disk_hash = format!("{:x}", Sha256::digest(content.as_bytes()));

    let mut warnings_count = 0;
    match db::templates::get_current_master_template(&state.db).await {
        Ok(Some(active_row)) => {
            if active_row.content_hash != disk_hash {
                warnings_count += 1;
                eprintln!(
                    "INTEGRITY WARNING: Active master_templates row hash ({}) differs from disk hash ({})!",
                    active_row.content_hash, disk_hash
                );
            } else {
                let hash_short = if disk_hash.len() >= 8 {
                    &disk_hash[..8]
                } else {
                    &disk_hash
                };
                println!(
                    "TRIPWIRE SUCCESS: Master template verified: active DB row matches {:?} (hash: {})",
                    master_file, hash_short
                );
            }
        }
        Ok(None) => {
            warnings_count += 1;
            eprintln!("INTEGRITY WARNING: No active row found in master_templates!");
        }
        Err(e) => {
            warnings_count += 1;
            eprintln!("Failed to query master_templates: {}", e);
        }
    }

    assert_eq!(warnings_count, 0, "Startup tripwire must log 0 warnings");
    println!("State hygiene verified: 0 warnings logged.");
    Ok(())
}
