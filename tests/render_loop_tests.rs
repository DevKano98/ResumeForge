use resumeforge::db;
use resumeforge::models::generation::{
    GeneratedExperienceItem, GeneratedProjectBullet, GeneratedProjectItem, GeneratedResumeContent,
    GeneratedSkills,
};
use resumeforge::services::event_bus::{self, EventBus};
use resumeforge::services::render_loop::{
    run_render_compile_loop, FinalResumeStatus, RenderLoopContext, RenderLoopOutcome,
};
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

fn sample_skills() -> GeneratedSkills {
    GeneratedSkills {
        languages: vec!["Rust".to_string(), "C++".to_string(), "Go".to_string()],
        frameworks_and_tools: vec![
            "Tokio".to_string(),
            "Axum".to_string(),
            "SQLite".to_string(),
        ],
        core_concepts: vec!["Distributed Systems".to_string(), "Concurrency".to_string()],
    }
}

#[tokio::test]
async fn test_render_loop_compaction_and_event_emission() -> anyhow::Result<()> {
    let tmp = TestDir::new("render");
    let db_path = tmp.path().join("test_render.db");
    let pool = db::init_db(&db_path).await?;
    let bus = EventBus::new();

    let resume = db::resumes::insert_resume(
        &pool,
        Some("Cloudflare"),
        Some("Senior Edge Engineer"),
        "Distributed edge computing, Rust, zero-copy",
        None,
        None,
        "rendering_latex",
    )
    .await?;
    let resume_id = resume.id;

    let ctx = RenderLoopContext {
        pool: &pool,
        bus: &bus,
        resume_id,
    };

    let classic_template = include_str!("../data/templates/starter/classic.tex");

    // Construct content that fits on 1 page
    let content = GeneratedResumeContent {
        summary: "Senior systems engineer with focus on distributed consensus, async runtimes, and memory-safe infrastructure.".to_string(),
        skills: sample_skills(),
        experience: vec![GeneratedExperienceItem {
            title: "Senior Systems Engineer".to_string(),
            company: "CloudScale Systems".to_string(),
            date_range: "Jan 2023 -- Present".to_string(),
            location: Some("Remote".to_string()),
            bullets: vec![
                "Architected high-throughput async processing engines handling over 50,000 requests per second with sub-millisecond p99 latency.".to_string(),
                "Reduced memory footprint by 35% by redesigning buffer allocation and zero-copy deserialization pipelines.".to_string(),
            ],
        }],
        projects: vec![GeneratedProjectItem {
            project_id: 101,
            name: "RaftConsensus".to_string(),
            bullets: vec![
                GeneratedProjectBullet {
                    text: "Engineered high-throughput Raft consensus loop in Rust with snapshot replication.".to_string(),
                    evidence_ids: vec![101],
                },
            ],
        }],
        achievements: vec![],
    };

    let compile_dir = tmp.path().join("compile");
    std::fs::create_dir_all(&compile_dir)?;
    let target_pdf = compile_dir.join("output.pdf");

    let keywords = vec![
        "rust".to_string(),
        "distributed".to_string(),
        "tokio".to_string(),
    ];

    let outcome = run_render_compile_loop(
        classic_template,
        &content,
        &keywords,
        &compile_dir,
        Some(&target_pdf),
        Some(&ctx),
    )
    .await?;

    match outcome {
        RenderLoopOutcome::Success {
            page_count,
            status,
            attempts,
            ..
        } => {
            assert_eq!(page_count, 1, "Must compile to exactly 1 page");
            assert!(
                status == FinalResumeStatus::Ready || status == FinalResumeStatus::ReadySparse,
                "Status must be Ready or ReadySparse"
            );
            assert!(attempts <= 4, "Must succeed within 4 attempts");
        }
        other => {
            panic!("Expected RenderLoopOutcome::Success, got: {:?}", other);
        }
    }

    // Verify events were emitted into generation_events
    let events = event_bus::get_events_for_resume(&pool, resume_id).await?;
    assert!(!events.is_empty(), "Render loop must emit events to event bus");

    Ok(())
}
