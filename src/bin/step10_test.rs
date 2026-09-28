use resumeforge::config::Config;
use resumeforge::db::resumes as db_resumes;
use resumeforge::routes::create_router;
use resumeforge::services::event_bus::{self, CanvasEvent};
use resumeforge::state::AppState;
use serde_json::json;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== ResumeForge Step 10 Test: Event Bus + Persistence + WebSocket Replay ===");

    // 1. Setup isolated test environment with SQLite
    let test_dir = std::env::current_dir()?.join("data").join("test_step10");
    let _ = tokio::fs::remove_dir_all(&test_dir).await;
    tokio::fs::create_dir_all(&test_dir).await?;

    let db_path = test_dir.join("test_resumeforge.db");
    let pool = resumeforge::db::init_db(&db_path).await?;

    let config = Config {
        port: 0,
        host: "127.0.0.1".to_string(),
        data_dir: test_dir.clone(),
        frontend_dir: std::env::current_dir()?.join("frontend"),
    };

    let state = AppState::new(config, pool.clone());
    let router = create_router(state.clone());

    // Bind Axum server to random local port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let local_addr = listener.local_addr()?;
    println!("Spawned test server on http://{}", local_addr);

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    // 2. Insert test resume row
    let resume = db_resumes::insert_resume(
        &pool,
        Some("TestFinTech"),
        Some("Staff Distributed Systems Engineer"),
        "Targeting low-latency async systems in Rust",
        Some("Emphasize Tokio and crash safety"),
        None,
        "generating_content",
    )
    .await?;

    let resume_id = resume.id;
    println!("Created test resume with ID: {}", resume_id);

    // 3. Emit initial 3 events into event bus & SQLite
    println!("\nEmitting initial 3 pipeline events...");
    let ev1 = event_bus::emit_event(
        &pool,
        &state.event_bus,
        resume_id,
        "jd_analysis",
        "command_started",
        json!({ "role": "Staff Distributed Systems Engineer", "core_skills": ["Rust", "Tokio", "SQLite"] }),
    )
    .await?;
    assert_eq!(ev1.seq, 1);

    let ev2 = event_bus::emit_event(
        &pool,
        &state.event_bus,
        resume_id,
        "project_retrieval",
        "command_output",
        json!({ "ranked_projects_count": 2, "top_score": 0.94 }),
    )
    .await?;
    assert_eq!(ev2.seq, 2);

    let ev3 = event_bus::emit_event(
        &pool,
        &state.event_bus,
        resume_id,
        "content_generation",
        "command_started",
        json!({ "conversation_id": "test-conv-1234", "tools_count": 0 }),
    )
    .await?;
    assert_eq!(ev3.seq, 3);

    println!("Verified initial events emitted with seq 1, 2, 3");

    // 4. Test REST Fallback: GET /api/resumes/{id}/events
    println!(
        "\nTesting REST fallback: GET /api/resumes/{}/events...",
        resume_id
    );
    let events_path = format!("/api/resumes/{}/events", resume_id);
    let json_body = get_http_json(local_addr, &events_path).await?;
    let rest_events: Vec<CanvasEvent> = serde_json::from_str(&json_body)?;
    assert_eq!(rest_events.len(), 3);
    assert_eq!(rest_events[0].seq, 1);
    assert_eq!(rest_events[1].seq, 2);
    assert_eq!(rest_events[2].seq, 3);
    println!("REST fallback returned all 3 events perfectly");

    // 5. Connect via raw WebSocket client: GET /ws/resumes/{id}/live
    println!(
        "\nConnecting WebSocket to ws://{}/ws/resumes/{}/live...",
        local_addr, resume_id
    );
    let mut ws_stream = connect_websocket(local_addr, resume_id).await?;
    println!("WebSocket handshake 101 Switching Protocols succeeded!");

    // Read replayed historical events from WebSocket
    let mut received_events = Vec::new();
    for _ in 0..3 {
        let frame_payload = read_ws_text_frame(&mut ws_stream).await?;
        let event: CanvasEvent = serde_json::from_str(&frame_payload)?;
        println!(
            "  [WS REPLAY] Seq {}: node='{}', type='{}'",
            event.seq, event.node, event.event_type
        );
        received_events.push(event);
    }
    assert_eq!(received_events.len(), 3);
    assert_eq!(received_events[0].seq, 1);
    assert_eq!(received_events[1].seq, 2);
    assert_eq!(received_events[2].seq, 3);
    println!("Historical replay over WebSocket verified (3 events replayed on connect)");

    // 6. Test LIVE broadcast streaming while WebSocket is actively connected
    println!("\nEmitting live events 4 and 5 while WS client is connected...");
    let pool_clone = pool.clone();
    let bus_clone = state.event_bus.clone();

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let _ = event_bus::emit_event(
            &pool_clone,
            &bus_clone,
            resume_id,
            "content_generation",
            "command_output",
            json!({ "chunk": "Senior Systems Engineer specializing in async Rust..." }),
        )
        .await;

        tokio::time::sleep(Duration::from_millis(50)).await;
        let _ = event_bus::emit_event(
            &pool_clone,
            &bus_clone,
            resume_id,
            "content_generation",
            "artifact_produced",
            json!({ "kind": "json", "summary": "Senior Systems Engineer..." }),
        )
        .await;
    });

    // Read live streamed events from WebSocket
    for _ in 0..2 {
        let frame_payload = read_ws_text_frame(&mut ws_stream).await?;
        let event: CanvasEvent = serde_json::from_str(&frame_payload)?;
        println!(
            "  [WS LIVE] Seq {}: node='{}', type='{}'",
            event.seq, event.node, event.event_type
        );
        received_events.push(event);
    }
    assert_eq!(received_events.len(), 5);
    assert_eq!(received_events[3].seq, 4);
    assert_eq!(received_events[4].seq, 5);
    drop(ws_stream);
    println!("Live WebSocket stream successfully received events 4 and 5 in real time!");

    // 7. Verify Reconnect / Page Refresh Replay: A new WS connection must replay all 5 events
    println!("\nTesting WS reconnect (simulating browser page refresh mid-generation)...");
    let mut ws_stream2 = connect_websocket(local_addr, resume_id).await?;
    let mut reconnected_events = Vec::new();
    for _ in 0..5 {
        let frame_payload = read_ws_text_frame(&mut ws_stream2).await?;
        let event: CanvasEvent = serde_json::from_str(&frame_payload)?;
        reconnected_events.push(event);
    }
    assert_eq!(reconnected_events.len(), 5);
    for i in 0..5 {
        assert_eq!(reconnected_events[i].seq, (i as i64) + 1);
    }
    drop(ws_stream2);
    println!("Page refresh replay verified: new connection immediately received full history of 5 events");

    // 8. Test Concurrent emit_event() Sequence Monotonicity (20 concurrent tasks)
    println!("\nTesting concurrent emit_event() with AtomicU64 sequence counter (20 concurrent calls)...");
    let conc_resume = db_resumes::insert_resume(
        &pool,
        Some("ConcurrentTestCorp"),
        Some("Concurrency Engineer"),
        "Testing high concurrency event emission",
        None,
        None,
        "queued",
    )
    .await?;
    let conc_resume_id = conc_resume.id;

    let mut tasks = Vec::new();
    for i in 0..20 {
        let pool_i = pool.clone();
        let bus_i = state.event_bus.clone();
        let task = tokio::spawn(async move {
            event_bus::emit_event(
                &pool_i,
                &bus_i,
                conc_resume_id,
                "concurrent_node",
                "command_output",
                json!({ "task_index": i }),
            )
            .await
        });
        tasks.push(task);
    }

    let mut emitted_seqs = Vec::new();
    for t in tasks {
        let event = t.await??;
        emitted_seqs.push(event.seq);
    }
    emitted_seqs.sort();
    println!(
        "Emitted seq values from 20 concurrent tasks: {:?}",
        emitted_seqs
    );

    // Verify all 20 seq values returned are unique and 1..=20
    let expected_seqs: Vec<i64> = (1..=20).collect();
    assert_eq!(
        emitted_seqs, expected_seqs,
        "Emitted seqs must be exactly 1..=20 with no gaps or duplicates"
    );

    // Verify when read back from SQLite directly
    let db_events = event_bus::get_events_for_resume(&pool, conc_resume_id).await?;
    assert_eq!(db_events.len(), 20, "SQLite must hold exactly 20 events");
    let read_seqs: Vec<i64> = db_events.iter().map(|e| e.seq).collect();
    assert_eq!(
        read_seqs, expected_seqs,
        "DB stored seqs must be strictly monotonic 1..=20 with zero duplicates or gaps"
    );
    println!("Verified 20 concurrent emit_event calls produced 100% unique sequence numbers (1..=20) in DB!");

    // 9. Verify agy stream-json mapper
    println!("\nTesting agy stream-json mapping helper...");
    let agy_init = json!({
        "event": "init",
        "init": {
            "conversation_id": "conv-test-999",
            "tools": ["ask_question", "run_command"]
        }
    });
    let (t1, p1) = event_bus::map_agy_event_to_canvas(&agy_init).unwrap();
    assert_eq!(t1, "command_started");
    assert_eq!(p1["conversation_id"], "conv-test-999");
    assert_eq!(p1["tools_count"], 2);

    let agy_delta = json!({
        "event": "step_update",
        "step_update": {
            "state": "ACTIVE",
            "text_delta": "Hello world chunk"
        }
    });
    let (t2, p2) = event_bus::map_agy_event_to_canvas(&agy_delta).unwrap();
    assert_eq!(t2, "command_output");
    assert_eq!(p2["chunk"], "Hello world chunk");

    let agy_result = json!({
        "event": "result",
        "result": {
            "status": "SUCCESS",
            "response": "{\"summary\": \"OK\"}"
        }
    });
    let (t3, p3) = event_bus::map_agy_event_to_canvas(&agy_result).unwrap();
    assert_eq!(t3, "artifact_produced");
    assert_eq!(p3["kind"], "json");
    assert_eq!(p3["content"], "{\"summary\": \"OK\"}");
    println!("Agy stream-json mapping helper verified against Section 7 schema");

    // 9. Cleanup test directory
    let _ = tokio::fs::remove_dir_all(&test_dir).await;

    println!("\n*** STEP 10 VERIFICATION PASSED: Event Bus + Persistence + WebSocket Replay 100% Functional ***");
    Ok(())
}

/// Helper function to perform raw RFC 6455 WebSocket client handshake over TcpStream
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

/// Helper function to read an unmasked WebSocket text frame from TcpStream
async fn read_ws_text_frame(stream: &mut TcpStream) -> anyhow::Result<String> {
    let mut header = [0u8; 2];
    stream.read_exact(&mut header).await?;

    let fin_opcode = header[0];
    if (fin_opcode & 0x0F) != 0x01 && (fin_opcode & 0x0F) != 0x02 {
        // Not text/binary
        anyhow::bail!("Unexpected WS opcode: {:#x}", fin_opcode);
    }

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

/// Helper function to perform raw HTTP GET request and return response body
async fn get_http_json(addr: SocketAddr, path: &str) -> anyhow::Result<String> {
    let mut stream = TcpStream::connect(addr).await?;
    let req = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        path, addr
    );
    stream.write_all(req.as_bytes()).await?;

    let mut response = String::new();
    stream.read_to_string(&mut response).await?;

    let parts: Vec<&str> = response.split("\r\n\r\n").collect();
    if parts.len() < 2 {
        anyhow::bail!("Invalid HTTP response: {}", response);
    }

    Ok(parts[1].to_string())
}
