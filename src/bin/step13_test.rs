use resumeforge::config::Config;
use resumeforge::db::resumes as db_resumes;
use resumeforge::models::generation::{
    GeneratedExperienceItem, GeneratedProjectBullet, GeneratedProjectItem, GeneratedResumeContent,
    GeneratedSkills,
};
use resumeforge::routes::create_router;
use resumeforge::services::render_loop::{
    run_stage_a_render_loop, RenderLoopContext, RenderLoopOutcome,
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
        "=== ResumeForge Step 13 Test: One-Page Validation, Shortening & Live Canvas Events ==="
    );
    ensure_toolchain_path();

    let temp_dir = PathBuf::from("data/temp/step13_test");
    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    tokio::fs::create_dir_all(&temp_dir).await?;

    // 0. Spin up SQLite & Axum Server with WebSocket support
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

    let resume = db_resumes::insert_resume(
        &pool,
        Some("Acme Infra"),
        Some("Principal Distributed Systems Engineer"),
        "Rust, Tokio, Storage engines",
        None,
        None,
        "rendering_latex",
    )
    .await?;
    let resume_id = resume.id;

    // Attach WebSocket client
    let mut ws_stream = connect_websocket(local_addr, resume_id).await?;
    println!(
        "  ✓ WebSocket client connected for live stream on resume_id: {}",
        resume_id
    );

    let template_src = tokio::fs::read_to_string("data/templates/starter/classic.tex").await?;
    let jd_keywords = vec![
        "Rust".to_string(),
        "Tokio".to_string(),
        "Distributed Systems".to_string(),
        "WAL".to_string(),
    ];

    // TEST 1: Naturally 1-page content
    println!("\n--- Test 1: Content that fits on 1 page immediately ---");
    let content_1page = GeneratedResumeContent {
        summary:
            "Distributed systems engineer specializing in high-throughput async Rust pipelines."
                .to_string(),
        skills: base_skills(),
        experience: vec![GeneratedExperienceItem {
            title: "Senior Systems Engineer".to_string(),
            company: "CloudScale Systems".to_string(),
            date_range: "2022 -- Present".to_string(),
            location: Some("Remote".to_string()),
            bullets: vec![
                "Architected high-throughput async processing engines handling 50k+ req/sec."
                    .to_string(),
                "Reduced memory footprint by 35% using zero-copy deserialization pipelines."
                    .to_string(),
            ],
        }],
        projects: vec![GeneratedProjectItem {
            project_id: 101,
            name: "ResilientQueue".to_string(),
            bullets: vec![GeneratedProjectBullet {
                text:
                    "Engineered crash-safe WAL transaction commit loop achieving 120,000 ops/sec."
                        .to_string(),
                evidence_ids: vec![1001],
            }],
        }],
        achievements: vec![],
    };

    let pdf_dest1 = temp_dir.join("test1_output.pdf");
    let res1 = run_stage_a_render_loop(
        &template_src,
        &content_1page,
        &jd_keywords,
        &temp_dir,
        Some(&pdf_dest1),
        None,
    )
    .await?;

    match res1 {
        RenderLoopOutcome::Success {
            attempts,
            dropped_bullets,
            page_count,
            content_density_pct,
            ..
        } => {
            println!("  ✓ Successfully passed on attempt #{}", attempts);
            assert_eq!(
                attempts, 1,
                "Should pass on attempt 1 without dropping bullets"
            );
            assert_eq!(dropped_bullets.len(), 0);
            assert_eq!(page_count, 1);
            println!("  ✓ Content density: {:.1}% (1 page)", content_density_pct);
        }
        other => panic!("Test 1 failed unexpectedly: {:?}", other),
    }

    // TEST 2: Content overflowing to 2 pages with live EventBus emission & WebSocket verification
    println!(
        "\n--- Test 2: Content overflowing by 1-2 bullets with WebSocket EventBus Verification ---"
    );
    let mut content_overflow = content_1page.clone();

    // Pre-populate near capacity boundary
    for i in 1..=22 {
        content_overflow.experience[0].bullets.push(format!(
            "Engineering responsibilities #{} delivering reliable async systems infrastructure in Rust.",
            i
        ));
    }

    let mut filler_count = 22;
    loop {
        let tex = resumeforge::services::latex::render_latex_template(
            &template_src,
            &content_overflow,
            false,
        )?;
        let probe_pdf = temp_dir.join("probe.pdf");
        let res =
            resumeforge::services::latex::compile_latex_content(&tex, &temp_dir, Some(&probe_pdf))
                .await?;
        if res.success && resumeforge::services::pdf::get_page_count(&probe_pdf)? == 2 {
            break;
        }
        filler_count += 1;
        content_overflow.experience[0].bullets.push(format!(
            "Engineering responsibilities #{} delivering reliable async systems infrastructure in Rust.",
            filler_count
        ));
    }

    let zero_rel_bullet =
        "Coordinated weekly catering menus and snack supply orders for office retreats."
            .to_string();
    content_overflow.experience[0]
        .bullets
        .push(zero_rel_bullet.clone());

    println!(
        "  Constructed test document with {} bullets that starts at 2 pages",
        content_overflow.experience[0].bullets.len()
    );

    let render_ctx = RenderLoopContext {
        pool: &pool,
        bus: &state.event_bus,
        resume_id,
    };

    let pdf_dest2 = temp_dir.join("test2_output.pdf");
    let res2 = run_stage_a_render_loop(
        &template_src,
        &content_overflow,
        &jd_keywords,
        &temp_dir,
        Some(&pdf_dest2),
        Some(&render_ctx),
    )
    .await?;

    match res2 {
        RenderLoopOutcome::Success {
            attempts,
            dropped_bullets,
            page_count,
            ..
        } => {
            println!(
                "  ✓ Stage A shortened successfully in {} attempts to exactly {} page!",
                attempts, page_count
            );
            assert!(
                attempts > 1,
                "Should have required at least 2 attempts to shorten"
            );
            assert_eq!(page_count, 1);
            assert!(
                !dropped_bullets.is_empty(),
                "Should have dropped at least 1 bullet"
            );

            println!("  ✓ Dropped bullets count: {}", dropped_bullets.len());
            for d in &dropped_bullets {
                println!(
                    "    - Attempt {}: [{}] '{}' (relevance: {})",
                    d.attempt, d.section, d.bullet_text, d.relevance_score
                );
                assert_eq!(
                    d.relevance_score, 0,
                    "Weakest relevance (score 0) bullets must be dropped first!"
                );
            }
        }
        other => panic!("Test 2 failed unexpectedly: {:?}", other),
    }

    // Read and print live WebSocket events
    println!("\n  --- Live WebSocket Canvas Events Received ---");
    let mut ws_events: Vec<serde_json::Value> = Vec::new();
    // We expect events until node_done
    loop {
        let frame = read_ws_text_frame(&mut ws_stream).await?;
        let event_json: serde_json::Value = serde_json::from_str(&frame)?;
        let event_type = event_json
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();
        let node = event_json
            .get("node")
            .and_then(|n| n.as_str())
            .unwrap_or("")
            .to_string();
        let seq = event_json.get("seq").and_then(|s| s.as_i64()).unwrap_or(0);
        let payload_str = event_json
            .get("payload")
            .unwrap_or(&serde_json::Value::Null)
            .to_string();

        println!(
            "    [WS seq: {}] node='{}' type='{}' payload={}",
            seq, node, event_type, payload_str
        );
        let is_done = event_type == "node_done" || event_type == "node_error";
        ws_events.push(event_json);

        if is_done {
            break;
        }
    }

    println!(
        "  ✓ Total live WebSocket events received: {}",
        ws_events.len()
    );
    assert!(
        !ws_events.is_empty(),
        "WebSocket must have received live events"
    );

    // Verify event sequence structure
    let event_types: Vec<&str> = ws_events
        .iter()
        .map(|e| e["type"].as_str().unwrap())
        .collect();
    assert!(
        event_types.contains(&"command_output"),
        "Must contain command_output events"
    );
    assert!(
        event_types.contains(&"artifact_produced"),
        "Must contain artifact_produced dropped bullet events"
    );
    assert_eq!(
        *event_types.last().unwrap(),
        "node_done",
        "Final event must be node_done"
    );

    // TEST 3: Extreme overflow exceeding 3 attempts -> Stage A PageLimitError
    println!("\n--- Test 3: Extreme overflow exceeding 3 attempts -> PageLimitError ---");
    let mut extreme_content = content_1page.clone();
    for i in 1..=35 {
        extreme_content.experience[0].bullets.push(format!(
            "Additional engineering duty #{} maintaining internal tooling.",
            i
        ));
    }

    let pdf_dest3 = temp_dir.join("test3_output.pdf");
    let res3 = run_stage_a_render_loop(
        &template_src,
        &extreme_content,
        &jd_keywords,
        &temp_dir,
        Some(&pdf_dest3),
        None,
    )
    .await?;

    match res3 {
        RenderLoopOutcome::PageLimitError {
            attempts,
            dropped_bullets,
            page_count,
            ..
        } => {
            println!("  ✓ Stage A correctly stopped at attempt #{} with PageLimitError (overflowed to {} pages)", attempts, page_count);
            assert_eq!(attempts, 3, "Stage A must halt after exactly 3 attempts");
            assert_eq!(
                dropped_bullets.len(),
                2,
                "Should have made 2 shortening drops across attempts 1->2 and 2->3"
            );
            assert!(page_count > 1);
        }
        other => panic!("Test 3 expected PageLimitError, got: {:?}", other),
    }

    let _ = tokio::fs::remove_dir_all(&temp_dir).await;

    println!("\n=== STEP 13 VERIFICATION PASSED: Stage A Shortening + EventBus & WebSocket stream verified! ===");
    Ok(())
}
