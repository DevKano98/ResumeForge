use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgyRunResult {
    pub exit_code: Option<i32>,
    pub response: String,
    pub denied_actions: Vec<String>,
    pub raw_events: Vec<serde_json::Value>,
    pub non_json_lines: Vec<String>,
    pub elapsed_ms: u64,
    pub status: AgyOutcome,
    pub error_detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgyOutcome {
    Success,
    AiEmptyResponse,
    AiToolDenied,
    AiTimeout,
    AiHung,
    AiInvalidJson,
    AiError,
}

pub async fn run_agy_prompt(
    prompt: &str,
    timeout_secs: u64,
) -> Result<AgyRunResult, anyhow::Error> {
    run_agy_prompt_with_events(prompt, timeout_secs, None).await
}

pub async fn run_agy_prompt_with_events(
    prompt: &str,
    timeout_secs: u64,
    events: Option<tokio::sync::mpsc::UnboundedSender<serde_json::Value>>,
) -> Result<AgyRunResult, anyhow::Error> {
    let prompt_owned = prompt.to_string();
    tokio::task::spawn_blocking(move || run_agy_prompt_inner(&prompt_owned, timeout_secs, events))
        .await?
}

pub fn run_agy_prompt_blocking(
    prompt: &str,
    timeout_secs: u64,
) -> Result<AgyRunResult, anyhow::Error> {
    run_agy_prompt_inner(prompt, timeout_secs, None)
}

fn run_agy_prompt_inner(
    prompt: &str,
    timeout_secs: u64,
    events: Option<tokio::sync::mpsc::UnboundedSender<serde_json::Value>>,
) -> Result<AgyRunResult, anyhow::Error> {
    let start = Instant::now();
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize {
        rows: 24,
        cols: 8192,
        pixel_width: 0,
        pixel_height: 0,
    })?;

    let mut cmd = CommandBuilder::new("agy");
    cmd.arg("-p");
    cmd.arg(prompt);
    cmd.arg("--output-format");
    cmd.arg("stream-json");
    let print_timeout = format!("{}s", timeout_secs);
    cmd.arg("--print-timeout");
    cmd.arg(&print_timeout);

    let mut child = pair.slave.spawn_command(cmd)?;
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader()?;
    let (tx, rx) = mpsc::channel::<Vec<u8>>();

    thread::spawn(move || {
        let mut buf = [0u8; 1024];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let mut collected = Vec::new();
    let mut live_pending = Vec::<u8>::new();
    let timeout_duration = Duration::from_secs(timeout_secs + 5);
    let mut child_exit_status = None;
    let mut exit_detected_time: Option<Instant> = None;
    let mut finished = false;
    let poll_interval = Duration::from_millis(50);

    while start.elapsed() < timeout_duration {
        while let Ok(chunk) = rx.try_recv() {
            push_live_events(&mut live_pending, &chunk, &events);
            collected.extend_from_slice(&chunk);
        }

        if child_exit_status.is_none() {
            match child.try_wait() {
                Ok(Some(status)) => {
                    child_exit_status = Some(status);
                    exit_detected_time = Some(Instant::now());
                }
                Ok(None) => {}
                Err(_) => {}
            }
        }

        // ConPTY non-EOF behavior: wait for process exit, then drain remaining pipe buffer for ~500ms
        if let Some(exit_time) = exit_detected_time {
            if exit_time.elapsed() >= Duration::from_millis(500) {
                while let Ok(chunk) = rx.try_recv() {
                    push_live_events(&mut live_pending, &chunk, &events);
                    collected.extend_from_slice(&chunk);
                }
                finished = true;
                break;
            }
        }

        thread::sleep(poll_interval);
    }

    let elapsed = start.elapsed();
    let mut timed_out = false;

    if !finished {
        timed_out = true;
        let _ = child.kill();
        let _ = child.wait();
        while let Ok(chunk) = rx.try_recv() {
            push_live_events(&mut live_pending, &chunk, &events);
            collected.extend_from_slice(&chunk);
        }
    }

    let exit_code = child_exit_status.map(|s| s.exit_code() as i32);
    let raw_text = String::from_utf8_lossy(&collected);

    let mut raw_events = Vec::new();
    let mut non_json_lines = Vec::new();
    let mut accumulated_deltas = String::new();
    let mut result_response: Option<String> = None;
    let mut denied_actions = Vec::new();

    let lines: Vec<&str> = raw_text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    let mut idx = 0;

    while idx < lines.len() {
        let mut buffer = String::new();
        let mut parsed_ok = false;
        let mut attempts = 0;
        let max_attempts = 10;

        while idx < lines.len() && attempts < max_attempts {
            if !buffer.is_empty() {
                buffer.push('\n');
            }
            buffer.push_str(lines[idx]);
            idx += 1;
            attempts += 1;

            match serde_json::from_str::<serde_json::Value>(&buffer) {
                Ok(val) => {
                    parsed_ok = true;
                    if let Some(event_type) = val.get("event").and_then(|e| e.as_str()) {
                        if event_type == "result" {
                            if let Some(result_obj) = val.get("result") {
                                if let Some(resp) =
                                    result_obj.get("response").and_then(|r| r.as_str())
                                {
                                    result_response = Some(resp.to_string());
                                }
                                if let Some(actions) =
                                    result_obj.get("denied_actions").and_then(|a| a.as_array())
                                {
                                    for action in actions {
                                        if let Some(act_str) =
                                            action.get("action").and_then(|s| s.as_str())
                                        {
                                            denied_actions.push(act_str.to_string());
                                        } else if let Some(disp) =
                                            action.get("display_name").and_then(|s| s.as_str())
                                        {
                                            denied_actions.push(disp.to_string());
                                        }
                                    }
                                }
                            }
                        } else if event_type == "step_update" {
                            if let Some(delta) = val
                                .pointer("/step_update/text_delta")
                                .and_then(|d| d.as_str())
                            {
                                accumulated_deltas.push_str(delta);
                            }
                        }
                    }
                    raw_events.push(val);
                    break;
                }
                Err(_) => {
                    // If buffer doesn't start with '{', it's a diagnostic line, break immediately
                    if !buffer.trim_start().starts_with('{') {
                        break;
                    }
                }
            }
        }

        if !parsed_ok {
            non_json_lines.push(buffer);
        }
    }

    let final_response = if let Some(resp) = result_response {
        if !resp.trim().is_empty() {
            resp
        } else {
            accumulated_deltas
        }
    } else {
        accumulated_deltas
    };

    // Outcome classification per Section 6
    let (status, error_detail) = if timed_out {
        (
            AgyOutcome::AiTimeout,
            Some(format!("Timed out after {}s", timeout_secs)),
        )
    } else if !denied_actions.is_empty() {
        (
            AgyOutcome::AiToolDenied,
            Some(format!(
                "Tool call denied by permission system: {}",
                denied_actions.join(", ")
            )),
        )
    } else if exit_code != Some(0) {
        (
            AgyOutcome::AiError,
            Some(format!(
                "Process exited with code {:?}. Non-JSON output: {}",
                exit_code,
                non_json_lines.join("; ")
            )),
        )
    } else if final_response.trim().is_empty() {
        (
            AgyOutcome::AiEmptyResponse,
            Some("Empty response received from Antigravity CLI".to_string()),
        )
    } else {
        (AgyOutcome::Success, None)
    };

    Ok(AgyRunResult {
        exit_code,
        response: final_response.trim().to_string(),
        denied_actions,
        raw_events,
        non_json_lines,
        elapsed_ms: elapsed.as_millis() as u64,
        status,
        error_detail,
    })
}

fn push_live_events(
    pending: &mut Vec<u8>,
    chunk: &[u8],
    sender: &Option<tokio::sync::mpsc::UnboundedSender<serde_json::Value>>,
) {
    let Some(sender) = sender else { return };
    pending.extend_from_slice(chunk);
    while let Some(end) = pending.iter().position(|byte| *byte == b'\n') {
        let line: Vec<u8> = pending.drain(..=end).collect();
        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&line) {
            let _ = sender.send(value);
        }
    }
    if pending.len() > 1_000_000 {
        pending.clear();
    }
}
