use resumeforge::config::Config;
use resumeforge::db::resumes as db_resumes;
use resumeforge::routes::create_router;
use resumeforge::services::event_bus;
use resumeforge::state::AppState;
use serde_json::json;
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== ResumeForge Step 11 Test: Minimal Canvas UI & Static File Serving ===");

    let test_dir = std::env::current_dir()?.join("data").join("test_step11");
    let _ = tokio::fs::remove_dir_all(&test_dir).await;
    tokio::fs::create_dir_all(&test_dir).await?;

    let db_path = test_dir.join("test_resumeforge.db");
    let pool = resumeforge::db::init_db(&db_path).await?;

    let frontend_dir = std::env::current_dir()?.join("frontend");
    let config = Config {
        port: 0,
        host: "127.0.0.1".to_string(),
        data_dir: test_dir.clone(),
        frontend_dir: frontend_dir.clone(),
    };

    let state = AppState::new(config, pool.clone());
    let router = create_router(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let local_addr = listener.local_addr()?;
    println!("Spawned server on http://{}", local_addr);

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    // 1. Verify static file serving
    println!("\n1. Verifying static file serving...");
    let resume_html = fetch_http(local_addr, "/resume.html").await?;
    assert!(
        resume_html.contains("ResumeForge — Orchestration Canvas"),
        "resume.html must contain title"
    );
    assert!(
        resume_html.contains("id=\"console-body\""),
        "resume.html must contain console container"
    );
    println!(
        "  ✓ /resume.html served correctly ({} bytes)",
        resume_html.len()
    );

    let app_css = fetch_http(local_addr, "/css/app.css").await?;
    assert!(
        app_css.contains("--bg-terminal"),
        "app.css must contain terminal styles"
    );
    println!(
        "  ✓ /css/app.css served correctly ({} bytes)",
        app_css.len()
    );

    let api_js = fetch_http(local_addr, "/js/api.js").await?;
    assert!(api_js.contains("getWsUrl"), "api.js must contain getWsUrl");
    println!("  ✓ /js/api.js served correctly ({} bytes)", api_js.len());

    let canvas_js = fetch_http(local_addr, "/js/canvas.js").await?;
    assert!(
        canvas_js.contains("appendEventToConsole"),
        "canvas.js must contain appendEventToConsole"
    );
    println!(
        "  ✓ /js/canvas.js served correctly ({} bytes)",
        canvas_js.len()
    );

    // 2. Insert test resume and emit simulated pipeline events
    println!("\n2. Emitting simulated agent pipeline events...");
    let resume = db_resumes::insert_resume(
        &pool,
        Some("Vercel"),
        Some("Staff Edge Systems Engineer"),
        "Rust, Tokio, WebAssembly, high-throughput proxy",
        None,
        None,
        "generating_content",
    )
    .await?;
    let resume_id = resume.id;

    // Emit events across different nodes
    event_bus::emit_event(
        &pool,
        &state.event_bus,
        resume_id,
        "jd_analysis",
        "command_started",
        json!({ "role": "Staff Edge Systems Engineer" }),
    )
    .await?;

    event_bus::emit_event(
        &pool,
        &state.event_bus,
        resume_id,
        "jd_analysis",
        "node_done",
        json!({ "core_skills": ["Rust", "Wasm", "Tokio"] }),
    )
    .await?;

    event_bus::emit_event(
        &pool,
        &state.event_bus,
        resume_id,
        "content_generation",
        "command_started",
        json!({ "conversation_id": "test-ai-step11", "tools_count": 0 }),
    )
    .await?;

    // 3. Connect WebSocket and verify events arrive in proper format
    println!("\n3. Connecting WebSocket client to verify event delivery for UI...");
    let mut stream = connect_websocket(local_addr, resume_id).await?;
    let mut ws_events = Vec::new();
    for _ in 0..3 {
        let frame = read_ws_text_frame(&mut stream).await?;
        let ev: serde_json::Value = serde_json::from_str(&frame)?;
        println!(
            "  ✓ Received event: seq={}, node={}, type={}",
            ev["seq"], ev["node"], ev["type"]
        );
        ws_events.push(ev);
    }

    assert_eq!(ws_events.len(), 3);
    assert_eq!(ws_events[0]["node"], "jd_analysis");
    assert_eq!(ws_events[1]["type"], "node_done");
    assert_eq!(ws_events[2]["node"], "content_generation");

    // Cleanup
    let _ = tokio::fs::remove_dir_all(&test_dir).await;

    println!("\n*** STEP 11 VERIFICATION PASSED: Minimal Canvas UI files served & WebSocket stream validated ***");
    Ok(())
}

async fn fetch_http(addr: SocketAddr, path: &str) -> anyhow::Result<String> {
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
    if !parts[0].contains("200 OK") {
        anyhow::bail!("HTTP status not 200 OK: {}", parts[0]);
    }
    Ok(parts[1].to_string())
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
