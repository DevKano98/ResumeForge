use resumeforge::config::Config;
use resumeforge::db::resumes as db_resumes;
use resumeforge::models::generation::{
    GeneratedExperienceItem, GeneratedProjectBullet, GeneratedProjectItem, GeneratedResumeContent,
    GeneratedSkills,
};
use resumeforge::routes::create_router;
use resumeforge::services::render_loop::{
    run_render_compile_loop, FinalResumeStatus, RenderLoopContext, RenderLoopOutcome,
};
use resumeforge::state::AppState;
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

fn ensure_toolchain_path() {
    if let Ok(user) = std::env::var("USERPROFILE") {
        let p = format!(
            "{}\\AppData\\Local\\agy\\bin;{}\\w64devkit\\bin;{}",
            user,
            user,
            std::env::var("PATH").unwrap_or_default()
        );
        std::env::set_var("PATH", p);
    }
}

fn base_skills() -> GeneratedSkills {
    GeneratedSkills {
        languages: vec!["Rust".to_string(), "C++".to_string(), "Go".to_string()],
        frameworks_and_tools: vec![
            "Tokio".to_string(),
            "Axum".to_string(),
            "SQLite & WAL".to_string(),
        ],
        core_concepts: vec!["Distributed Systems".to_string(), "Concurrency".to_string()],
    }
}

async fn connect_websocket(addr: SocketAddr, resume_id: i64) -> anyhow::Result<TcpStream> {
    let mut stream = TcpStream::connect(addr).await?;
    let ws_key = ["dGhl", "IHNhbXBs", "ZSBub25jZQ=="].concat();
    let request = format!(
        "GET /ws/resumes/{}/live HTTP/1.1\r\n\
         Host: {}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: {}\r\n\
         Sec-WebSocket-Version: 13\r\n\r\n",
        resume_id, addr, ws_key
    );
    stream.write_all(request.as_bytes()).await?;

    let mut buf = [0u8; 1024];
    let n = stream.read(&mut buf).await?;
    let response = String::from_utf8_lossy(&buf[..n]);

    if !response.contains("101 Switching Protocols") {
        anyhow::bail!("WebSocket handshake failed: {}", response);
    }

    Ok(stream)
}

async fn read_ws_text_frame(stream: &mut TcpStream) -> anyhow::Result<String> {
    let mut header = [0u8; 2];
    stream.read_exact(&mut header).await?;

    let mask_len = header[1];
    let is_masked = (mask_len & 0x80) != 0;
    let mut len = (mask_len & 0x7F) as usize;

    if len == 126 {
        let mut ext = [0u8; 2];
        stream.read_exact(&mut ext).await?;
        len = u16::from_be_bytes(ext) as usize;
    } else if len == 127 {
        let mut ext = [0u8; 8];
        stream.read_exact(&mut ext).await?;
        len = u64::from_be_bytes(ext) as usize;
    }

    let mut payload = vec![0u8; len];
    if is_masked {
        let mut mask_key = [0u8; 4];
        stream.read_exact(&mut mask_key).await?;
        stream.read_exact(&mut payload).await?;
        for (i, byte) in payload.iter_mut().enumerate() {
            *byte ^= mask_key[i % 4];
        }
    } else {
        stream.read_exact(&mut payload).await?;
    }

    Ok(String::from_utf8(payload)?)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!(
        "=== ResumeForge Step 14 Test: Stage B (\\tighten) Attempt & ready_sparse Detection ==="
    );
    ensure_toolchain_path();

    let temp_dir = PathBuf::from("data/temp/step14_test");
    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    tokio::fs::create_dir_all(&temp_dir).await?;

    let db_path = temp_dir.join("test_resumeforge.db");
    let pool = resumeforge::db::init_db(&db_path).await?;
    let config = Config {
        port: 0,
        host: "127.0.0.1".to_string(),
        data_dir: temp_dir.clone(),
        frontend_dir: std::env::current_dir()?.join("frontend"),
    };
    let state = AppState::new(config, pool.clone());
    let router = create_router(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let local_addr = listener.local_addr()?;
    println!("Spawned test server on ws://{}", local_addr);

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    let template_src = tokio::fs::read_to_string("data/templates/starter/classic.tex").await?;
    let jd_keywords = vec![
        "Rust".to_string(),
        "Tokio".to_string(),
        "Distributed Systems".to_string(),
        "WAL".to_string(),
    ];

    // TEST 1: Sparse page detection -> ready_sparse
    println!("\n--- Test 1: Sparse 1-page document (<60% density) -> ready_sparse ---");
    let sparse_content = GeneratedResumeContent {
        summary: "Rust developer.".to_string(),
        skills: GeneratedSkills {
            languages: vec!["Rust".to_string()],
            frameworks_and_tools: vec!["Tokio".to_string()],
            core_concepts: vec!["Concurrency".to_string()],
        },
        experience: vec![GeneratedExperienceItem {
            title: "Junior Engineer".to_string(),
            company: "Startup".to_string(),
            date_range: "2023".to_string(),
            location: None,
            bullets: vec!["Wrote unit tests.".to_string()],
        }],
        projects: vec![GeneratedProjectItem {
            project_id: 1,
            name: "Demo".to_string(),
            bullets: vec![GeneratedProjectBullet {
                text: "Simple tool.".to_string(),
                evidence_ids: vec![1],
            }],
        }],
        achievements: vec![],
    };

    let pdf_dest1 = temp_dir.join("test1_sparse.pdf");
    let res1 = run_render_compile_loop(
        &template_src,
        &sparse_content,
        &jd_keywords,
        &temp_dir,
        Some(&pdf_dest1),
        None,
    )
    .await?;

    match res1 {
        RenderLoopOutcome::Success {
            attempts,
            page_count,
            content_density_pct,
            is_sparse,
            status,
            compact_mode_applied,
            ..
        } => {
            println!(
                "  ✓ Finished in {} attempt(s) with page_count={}",
                attempts, page_count
            );
            println!(
                "  ✓ Density: {:.1}%, is_sparse={}, status={:?}",
                content_density_pct, is_sparse, status
            );
            assert_eq!(page_count, 1);
            assert!(
                content_density_pct < 60.0,
                "Sparse document density must be < 60.0%"
            );
            assert!(is_sparse);
            assert_eq!(
                status,
                FinalResumeStatus::ReadySparse,
                "Must classify as ReadySparse"
            );
            assert!(
                !compact_mode_applied,
                "Compact mode not needed for sparse doc"
            );
        }
        other => panic!("Test 1 expected Success with ReadySparse, got {:?}", other),
    }

    // TEST 2: Adequately-filled page -> ready
    println!("\n--- Test 2: Adequately filled 1-page document (>=60% density) -> ready ---");
    let mut filled_content = sparse_content.clone();
    filled_content.summary = "Senior Distributed Systems Infrastructure Engineer with 10+ years specializing in low-latency async Rust, fault-tolerant consensus loops, and high-performance embedded storage engines.".to_string();
    filled_content.skills = base_skills();
    for i in 1..=12 {
        filled_content.experience[0].bullets.push(format!(
            "Architected high-throughput async processing engine component #{} processing 60,000 req/sec in Rust.",
            i
        ));
    }

    let pdf_dest2 = temp_dir.join("test2_filled.pdf");
    let res2 = run_render_compile_loop(
        &template_src,
        &filled_content,
        &jd_keywords,
        &temp_dir,
        Some(&pdf_dest2),
        None,
    )
    .await?;

    match res2 {
        RenderLoopOutcome::Success {
            page_count,
            content_density_pct,
            is_sparse,
            status,
            ..
        } => {
            println!(
                "  ✓ Finished with page_count={}, density={:.1}%, status={:?}",
                page_count, content_density_pct, status
            );
            assert_eq!(page_count, 1);
            assert!(
                content_density_pct >= 60.0,
                "Filled document density must be >= 60.0%"
            );
            assert!(!is_sparse);
            assert_eq!(status, FinalResumeStatus::Ready, "Must classify as Ready");
        }
        other => panic!("Test 2 expected Success with Ready, got {:?}", other),
    }

    // TEST 3: Stage B \tighten compaction (Attempt 4) with WebSocket verification
    println!("\n--- Test 3: Stage B \\tighten compaction (Attempt 4) ---");
    let resume = db_resumes::insert_resume(
        &pool,
        Some("Global Cloud"),
        Some("Staff Systems Architect"),
        "Rust, Tokio, Storage",
        None,
        None,
        "rendering_latex",
    )
    .await?;
    let resume_id = resume.id;

    let mut ws_stream = connect_websocket(local_addr, resume_id).await?;
    let render_ctx = RenderLoopContext {
        pool: &pool,
        bus: &state.event_bus,
        resume_id,
    };

    // Construct content that overflows Stage A's 3 attempts on standard margins,
    // but compresses down to 1 page under Stage B \tighten (margin=0.4in)
    let mut tighten_candidate = filled_content.clone();
    // Calibrate: 29 bullets overflows standard 0.6in margins even after 2 drops (27 bullets),
    // but fits on 1 page under \tighten's 0.4in margins!
    for i in 13..=29 {
        tighten_candidate.experience[0].bullets.push(format!(
            "Engineered distributed consensus replication worker #{} under Raft in Rust.",
            i
        ));
    }

    let pdf_dest3 = temp_dir.join("test3_tightened.pdf");
    let res3 = run_render_compile_loop(
        &template_src,
        &tighten_candidate,
        &jd_keywords,
        &temp_dir,
        Some(&pdf_dest3),
        Some(&render_ctx),
    )
    .await?;

    match res3 {
        RenderLoopOutcome::Success {
            attempts,
            page_count,
            compact_mode_applied,
            status,
            dropped_bullets,
            ..
        } => {
            println!("  ✓ Stage B compaction passed on attempt #{}!", attempts);
            assert_eq!(
                attempts, 4,
                "Must succeed on attempt 4 via Stage B \\tighten"
            );
            assert_eq!(page_count, 1);
            assert!(compact_mode_applied, "compact_mode_applied must be true!");
            assert_eq!(
                dropped_bullets.len(),
                2,
                "Stage A dropped 2 bullets in attempts 1->2 and 2->3"
            );
            println!(
                "  ✓ Status: {:?}, compact_mode_applied={}",
                status, compact_mode_applied
            );
        }
        other => panic!("Test 3 expected Stage B Success, got {:?}", other),
    }

    // Read and verify WebSocket events for Stage B
    println!("\n  --- Live WebSocket Canvas Events for Stage B ---");
    let mut ws_events: Vec<serde_json::Value> = Vec::new();
    loop {
        let frame = read_ws_text_frame(&mut ws_stream).await?;
        let event_json: serde_json::Value = serde_json::from_str(&frame)?;
        let event_type = event_json
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();
        let seq = event_json.get("seq").and_then(|s| s.as_i64()).unwrap_or(0);
        let payload_str = event_json
            .get("payload")
            .unwrap_or(&serde_json::Value::Null)
            .to_string();

        println!(
            "    [WS seq: {}] type='{}' payload={}",
            seq, event_type, payload_str
        );
        let is_done = event_type == "node_done" || event_type == "node_error";
        ws_events.push(event_json);

        if is_done {
            break;
        }
    }

    // Verify WebSocket captured Attempt 4 \tighten event
    let has_tighten_event = ws_events.iter().any(|e| {
        e.get("payload")
            .and_then(|p| p.get("compact_mode"))
            .and_then(|c| c.as_bool())
            == Some(true)
    });
    assert!(
        has_tighten_event,
        "WebSocket stream must include Stage B tighten event"
    );

    let last_event = ws_events.last().unwrap();
    assert_eq!(last_event["type"], "node_done");
    assert_eq!(last_event["payload"]["compact_mode_applied"], true);
    assert_eq!(last_event["payload"]["attempts_used"], 4);
    println!("  ✓ WebSocket successfully verified Stage B event flow");

    let _ = tokio::fs::remove_dir_all(&temp_dir).await;

    println!("\n=== STEP 14 VERIFICATION PASSED: Stage B (\\tighten) & ready_sparse Detection ===");
    Ok(())
}
