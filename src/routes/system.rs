use crate::services::command_runner::run_command;
use crate::services::pty_runner::{run_agy_prompt, AgyOutcome};
use crate::state::{AppState, CachedAgyProbe};
use axum::{extract::State, Json};
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize)]
pub struct SystemStatusResponse {
    pub sqlite: SqliteStatus,
    pub gh: GhStatus,
    pub agy: AgyStatus,
    pub tectonic: TectonicStatus,
    pub gitleaks: GitleaksStatus,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitleaksStatus {
    pub installed: bool,
    pub version: Option<String>,
    pub details: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SqliteStatus {
    pub connected: bool,
    pub foreign_keys_enabled: bool,
    pub details: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GhStatus {
    pub installed: bool,
    pub authenticated: bool,
    pub username: Option<String>,
    pub details: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AgyStatus {
    pub installed: bool,
    pub outcome: AgyOutcome,
    pub response: String,
    pub elapsed_ms: u64,
    pub denied_actions: Vec<String>,
    pub details: String,
    pub checked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TectonicStatus {
    pub installed: bool,
    pub version: Option<String>,
    pub bundle_cached: bool,
    pub cache_path: Option<String>,
    pub details: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AntigravityProbeResponse {
    pub outcome: AgyOutcome,
    pub response: String,
    pub elapsed_ms: u64,
    pub denied_actions: Vec<String>,
    pub exit_code: Option<i32>,
    pub details: String,
    pub checked_at: DateTime<Utc>,
}

pub async fn get_system_status(State(state): State<AppState>) -> Json<SystemStatusResponse> {
    // 1. SQLite Status (live check: pool query takes <1ms)
    let sqlite = check_sqlite(&state).await;

    // 2. GitHub CLI Status (live check: `gh auth status` local subprocess takes ~15ms)
    // Note: We evaluate `gh` and `tectonic` live because local CLI detection is lightweight (<20ms),
    // while Antigravity's model turn takes 13-16s and is read from cache below.
    let gh = check_gh().await;

    // 3. Antigravity Probe (read from memory cache: responds in microseconds, never spawns live PTY)
    let agy = get_cached_agy_status(&state).await;

    // 4. Tectonic Status (live check: binary check & directory scan takes <5ms)
    let tectonic = check_tectonic(&state.config.data_dir).await;

    // 5. Gitleaks Status (live check: binary version check takes <15ms)
    let gitleaks = check_gitleaks().await;

    Json(SystemStatusResponse {
        sqlite,
        gh,
        agy,
        tectonic,
        gitleaks,
    })
}

pub async fn get_antigravity_probe(
    State(state): State<AppState>,
) -> Json<AntigravityProbeResponse> {
    let probe_res = run_agy_prompt("reply with the single word OK", 30).await;
    let checked_at = Utc::now();

    match probe_res {
        Ok(res) => {
            let details = match &res.status {
                AgyOutcome::Success => {
                    if res.response.trim().eq_ignore_ascii_case("ok") {
                        format!(
                            "Responded cleanly with '{}' in {}ms",
                            res.response.trim(),
                            res.elapsed_ms
                        )
                    } else {
                        format!(
                            "Responded with '{}' (expected 'OK') in {}ms",
                            res.response.trim(),
                            res.elapsed_ms
                        )
                    }
                }
                AgyOutcome::AiEmptyResponse => {
                    "Process exited 0 but produced zero response bytes".to_string()
                }
                AgyOutcome::AiToolDenied => format!(
                    "Unauthorized tool access denied: {}",
                    res.denied_actions.join(", ")
                ),
                AgyOutcome::AiTimeout => format!("Timed out after {}ms", res.elapsed_ms),
                AgyOutcome::AiHung => format!("Hung and terminated after {}ms", res.elapsed_ms),
                AgyOutcome::AiInvalidJson => "Produced invalid JSON".to_string(),
                AgyOutcome::AiError => res
                    .error_detail
                    .clone()
                    .unwrap_or_else(|| "Process exited with error".to_string()),
            };

            let cached = CachedAgyProbe {
                outcome: res.status.clone(),
                response: res.response.clone(),
                elapsed_ms: res.elapsed_ms,
                denied_actions: res.denied_actions.clone(),
                details: details.clone(),
                checked_at,
            };

            // Update shared cache
            {
                let mut write_lock = state.agy_cache.write().await;
                *write_lock = Some(cached);
            }

            Json(AntigravityProbeResponse {
                outcome: res.status,
                response: res.response,
                elapsed_ms: res.elapsed_ms,
                denied_actions: res.denied_actions,
                exit_code: res.exit_code,
                details,
                checked_at,
            })
        }
        Err(e) => {
            let details = format!("Failed to spawn Antigravity PTY runner: {}", e);
            let cached = CachedAgyProbe {
                outcome: AgyOutcome::AiError,
                response: String::new(),
                elapsed_ms: 0,
                denied_actions: Vec::new(),
                details: details.clone(),
                checked_at,
            };

            {
                let mut write_lock = state.agy_cache.write().await;
                *write_lock = Some(cached);
            }

            Json(AntigravityProbeResponse {
                outcome: AgyOutcome::AiError,
                response: String::new(),
                elapsed_ms: 0,
                denied_actions: Vec::new(),
                exit_code: None,
                details,
                checked_at,
            })
        }
    }
}

async fn get_cached_agy_status(state: &AppState) -> AgyStatus {
    let cache_lock = state.agy_cache.read().await;
    match &*cache_lock {
        Some(cached) => AgyStatus {
            installed: true,
            outcome: cached.outcome.clone(),
            response: cached.response.clone(),
            elapsed_ms: cached.elapsed_ms,
            denied_actions: cached.denied_actions.clone(),
            details: cached.details.clone(),
            checked_at: Some(cached.checked_at),
        },
        None => AgyStatus {
            installed: false,
            outcome: AgyOutcome::AiEmptyResponse,
            response: String::new(),
            elapsed_ms: 0,
            denied_actions: Vec::new(),
            details: "Probe has not been run yet".to_string(),
            checked_at: None,
        },
    }
}

async fn check_sqlite(state: &AppState) -> SqliteStatus {
    match sqlx::query_scalar::<_, i64>("PRAGMA foreign_keys;")
        .fetch_one(&state.db)
        .await
    {
        Ok(fk) => SqliteStatus {
            connected: true,
            foreign_keys_enabled: fk == 1,
            details: if fk == 1 {
                "Connected with foreign keys enforced (PRAGMA foreign_keys = 1)".to_string()
            } else {
                "Connected, but foreign keys are NOT enabled".to_string()
            },
        },
        Err(e) => SqliteStatus {
            connected: false,
            foreign_keys_enabled: false,
            details: format!("SQLite connection error: {}", e),
        },
    }
}

async fn check_gh() -> GhStatus {
    match run_command("gh", &["auth", "status"]).await {
        Ok(output) => {
            let combined = format!(
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );

            let authenticated = output.status.success();
            let username = if authenticated || combined.contains("Logged in to") {
                let re = Regex::new(r"account\s+([A-Za-z0-9_-]+)").ok();
                re.and_then(|r| {
                    r.captures(&combined)
                        .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
                })
            } else {
                None
            };

            GhStatus {
                installed: true,
                authenticated,
                username,
                details: if authenticated {
                    "Authenticated with GitHub CLI".to_string()
                } else {
                    "GitHub CLI installed but not authenticated. Run `gh auth login`.".to_string()
                },
            }
        }
        Err(_) => GhStatus {
            installed: false,
            authenticated: false,
            username: None,
            details: "GitHub CLI (`gh`) not found in PATH".to_string(),
        },
    }
}

async fn check_tectonic(data_dir: &Path) -> TectonicStatus {
    let version = match run_command("tectonic", &["--version"]).await {
        Ok(out) if out.status.success() => {
            let line = String::from_utf8_lossy(&out.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            Some(line)
        }
        _ => None,
    };

    let installed = version.is_some();
    let (bundle_cached, cache_path) = detect_tectonic_bundle(data_dir);

    let details = match (&version, bundle_cached) {
        (Some(v), true) => format!("Installed ({}), bundle cached locally", v),
        (Some(v), false) => format!(
            "Installed ({}), but bundle is not cached locally (will download on first compile)",
            v
        ),
        (None, _) => "Tectonic executable not found in PATH".to_string(),
    };

    TectonicStatus {
        installed,
        version,
        bundle_cached,
        cache_path: cache_path.map(|p| p.to_string_lossy().to_string()),
        details,
    }
}

fn detect_tectonic_bundle(data_dir: &Path) -> (bool, Option<PathBuf>) {
    // 1. data/tectonic-cache/
    let local_cache = data_dir.join("tectonic-cache");
    if local_cache.exists()
        && std::fs::read_dir(&local_cache)
            .map(|mut d| d.next().is_some())
            .unwrap_or(false)
    {
        return (true, Some(local_cache));
    }

    // 2. %LOCALAPPDATA%\TectonicProject\Tectonic\cache or %LOCALAPPDATA%\Tectonic\cache
    // TODO (Section 12, Step 27 - packaging): Cross-platform bundle cache detection paths:
    // - Windows: %LOCALAPPDATA%\TectonicProject\Tectonic\cache
    // - macOS:   ~/Library/Caches/TectonicProject/Tectonic/cache
    // - Linux:   ~/.cache/TectonicProject/Tectonic/cache or $XDG_CACHE_HOME/TectonicProject
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        let p1 = PathBuf::from(&local_app_data)
            .join("TectonicProject")
            .join("Tectonic")
            .join("cache");
        if p1.exists()
            && std::fs::read_dir(&p1)
                .map(|mut d| d.next().is_some())
                .unwrap_or(false)
        {
            return (true, Some(p1));
        }

        let p2 = PathBuf::from(&local_app_data)
            .join("Tectonic")
            .join("cache");
        if p2.exists()
            && std::fs::read_dir(&p2)
                .map(|mut d| d.next().is_some())
                .unwrap_or(false)
        {
            return (true, Some(p2));
        }
    }

    // 3. Unix ~/.cache/tectonic
    if let Ok(home) = std::env::var("HOME") {
        let p = PathBuf::from(home).join(".cache").join("tectonic");
        if p.exists()
            && std::fs::read_dir(&p)
                .map(|mut d| d.next().is_some())
                .unwrap_or(false)
        {
            return (true, Some(p));
        }
    }

    (false, None)
}

async fn check_gitleaks() -> GitleaksStatus {
    match run_command("gitleaks", &["version"]).await {
        Ok(out) if out.status.success() => {
            let ver = String::from_utf8_lossy(&out.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            let version_str = if ver.is_empty() { None } else { Some(ver.clone()) };
            GitleaksStatus {
                installed: true,
                version: version_str,
                details: format!("Installed ({})", ver),
            }
        }
        _ => GitleaksStatus {
            installed: false,
            version: None,
            details: "gitleaks executable not found in PATH".to_string(),
        },
    }
}
