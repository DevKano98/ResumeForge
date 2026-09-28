use crate::config::Config;
use crate::services::pty_runner::AgyOutcome;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedAgyProbe {
    pub outcome: AgyOutcome,
    pub response: String,
    pub elapsed_ms: u64,
    pub denied_actions: Vec<String>,
    pub details: String,
    pub checked_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: SqlitePool,
    pub agy_cache: Arc<RwLock<Option<CachedAgyProbe>>>,
    pub event_bus: crate::services::event_bus::EventBus,
    pub generation_tx: mpsc::Sender<i64>,
}

impl AppState {
    pub fn new(config: Config, db: SqlitePool) -> Self {
        let (generation_tx, generation_rx) = mpsc::channel(128);
        let config = Arc::new(config);
        let event_bus = crate::services::event_bus::EventBus::new();
        crate::services::generation_queue::start_worker(
            generation_rx,
            config.clone(),
            db.clone(),
            event_bus.clone(),
        );
        Self {
            config,
            db,
            agy_cache: Arc::new(RwLock::new(None)),
            event_bus,
            generation_tx,
        }
    }
}
