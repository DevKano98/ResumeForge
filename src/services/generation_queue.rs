use crate::config::Config;
use crate::services::{event_bus, resume_generator};
use sqlx::SqlitePool;
use std::future::Future;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Await each ID on the sole receiver so generations cannot overlap.
async fn consume_queue<F, Fut>(mut rx: mpsc::Receiver<i64>, mut process: F)
where
    F: FnMut(i64) -> Fut,
    Fut: Future<Output = ()>,
{
    while let Some(id) = rx.recv().await {
        process(id).await;
    }
}

pub fn start_worker(
    rx: mpsc::Receiver<i64>,
    config: Arc<Config>,
    db: SqlitePool,
    bus: event_bus::EventBus,
) {
    tokio::spawn(async move {
        consume_queue(rx, |id| {
            let config = config.clone();
            let db = db.clone();
            let bus = bus.clone();
            async move {
                // A queued row may have been deleted before the worker reaches it.
                let claimed = sqlx::query("UPDATE resumes SET status = 'analysing_job' WHERE id = ? AND status = 'queued'")
                    .bind(id).execute(&db).await;
                let Ok(claimed) = claimed else {
                    tracing::error!(resume_id = id, "Could not claim queued resume");
                    return;
                };
                if claimed.rows_affected() == 0 { return; }
                if let Err(err) = resume_generator::run(id, &config, &db, &bus).await {
                    tracing::error!(resume_id = id, error = %err, "Queued generation failed");
                    let update = sqlx::query(
                        "UPDATE resumes SET status = ?, error_stage = ?, \
                         error_detail = ?, completed_at = datetime('now') \
                         WHERE id = ? AND status NOT IN ('ready', 'ready_sparse', 'cancelled')"
                    )
                    .bind(err.status)
                    .bind(err.stage)
                    .bind(&err.detail)
                    .bind(id)
                    .execute(&db)
                    .await;
                    match update {
                        Ok(done) if done.rows_affected() > 0 => {
                            let _ = event_bus::emit_event(&db, &bus, id, err.stage, "node_error",
                                serde_json::json!({"stage": err.status, "detail": err.detail, "timeout_secs": err.timeout_secs})).await;
                        }
                        Ok(_) => {}
                        Err(update_err) => tracing::error!(resume_id = id, error = %update_err, "Could not record generation failure"),
                    }
                }
            }
        })
        .await;
    });
}

#[cfg(test)]
mod tests {
    use super::consume_queue;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::sync::mpsc;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn multiple_jobs_never_overlap() {
        let (tx, rx) = mpsc::channel(128);
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let completed = Arc::new(AtomicUsize::new(0));
        let worker = tokio::spawn({
            let active = active.clone();
            let peak = peak.clone();
            let completed = completed.clone();
            async move {
                consume_queue(rx, move |_| {
                    let active = active.clone();
                    let peak = peak.clone();
                    let completed = completed.clone();
                    async move {
                        let running = active.fetch_add(1, Ordering::SeqCst) + 1;
                        peak.fetch_max(running, Ordering::SeqCst);
                        sleep(Duration::from_millis(10)).await;
                        active.fetch_sub(1, Ordering::SeqCst);
                        completed.fetch_add(1, Ordering::SeqCst);
                    }
                })
                .await;
            }
        });
        for id in 1..=8 {
            tx.send(id).await.unwrap();
        }
        drop(tx);
        worker.await.unwrap();
        assert_eq!(completed.load(Ordering::SeqCst), 8);
        assert_eq!(peak.load(Ordering::SeqCst), 1);
    }
}
