use resumeforge::db;
use resumeforge::services::event_bus::{self, EventBus};
use serde_json::json;
use std::path::{Path, PathBuf};

struct TestDir(PathBuf);
impl TestDir {
    fn new(prefix: &str) -> Self {
        let p = std::env::temp_dir().join(format!("rf_test_{}_{}", prefix, uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&p);
        Self(p)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn test_event_bus_sequencing_persistence_and_broadcast() -> anyhow::Result<()> {
    let tmp = TestDir::new("events");
    let db_path = tmp.path().join("test_events.db");
    let pool = db::init_db(&db_path).await?;

    let bus = EventBus::new();

    // Create a resume to associate events with
    let resume = db::resumes::insert_resume(
        &pool,
        Some("Stripe"),
        Some("Staff Systems Engineer"),
        "Distributed systems, high availability, Rust",
        None,
        None,
        "generating_content",
    )
    .await?;
    let resume_id = resume.id;

    // Subscribe before emitting
    let mut rx = bus.subscribe(resume_id).await;

    // Emit event 1
    let ev1 = event_bus::emit_event(
        &pool,
        &bus,
        resume_id,
        "jd_analysis",
        "command_started",
        json!({"message": "Parsing JD"}),
    )
    .await?;
    assert_eq!(ev1.seq, 1);
    assert_eq!(ev1.node, "jd_analysis");
    assert_eq!(ev1.event_type, "command_started");

    // Receive broadcast 1
    let received1 = rx.recv().await?;
    assert_eq!(received1.seq, 1);
    assert_eq!(received1.node, "jd_analysis");

    // Emit event 2
    let ev2 = event_bus::emit_event(
        &pool,
        &bus,
        resume_id,
        "jd_analysis",
        "node_done",
        json!({"role": "Staff Systems Engineer", "core_skills": ["Rust", "Distributed Systems"]}),
    )
    .await?;
    assert_eq!(ev2.seq, 2);

    // Receive broadcast 2
    let received2 = rx.recv().await?;
    assert_eq!(received2.seq, 2);

    // Emit event 3 on different node
    let ev3 = event_bus::emit_event(
        &pool,
        &bus,
        resume_id,
        "project_retrieval",
        "command_started",
        json!({"message": "Ranking projects"}),
    )
    .await?;
    assert_eq!(ev3.seq, 3);

    // Verify DB persistence and ordering
    let persisted = event_bus::get_events_for_resume(&pool, resume_id).await?;
    assert_eq!(persisted.len(), 3);
    assert_eq!(persisted[0].seq, 1);
    assert_eq!(persisted[0].node, "jd_analysis");
    assert_eq!(persisted[1].seq, 2);
    assert_eq!(persisted[2].seq, 3);
    assert_eq!(persisted[2].node, "project_retrieval");

    Ok(())
}
