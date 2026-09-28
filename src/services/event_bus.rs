use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

/// Normalized Canvas Event Schema matching Section 7 specifications
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasEvent {
    pub resume_id: i64,
    pub node: String,
    pub seq: i64,
    pub ts: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct GenerationEventRow {
    pub id: i64,
    pub resume_id: i64,
    pub seq: i64,
    pub ts: String,
    pub node: String,
    pub event_type: String,
    pub payload_json: String,
}

use std::sync::atomic::{AtomicU64, Ordering};

/// In-memory broadcast event bus managing per-resume channels and sequence counters
#[derive(Clone)]
pub struct EventBus {
    senders: Arc<RwLock<HashMap<i64, broadcast::Sender<CanvasEvent>>>>,
    counters: Arc<RwLock<HashMap<i64, Arc<AtomicU64>>>>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            senders: Arc::new(RwLock::new(HashMap::new())),
            counters: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Subscribes to live events for a specific resume_id
    pub async fn subscribe(&self, resume_id: i64) -> broadcast::Receiver<CanvasEvent> {
        let mut senders = self.senders.write().await;
        let sender = senders.entry(resume_id).or_insert_with(|| {
            let (tx, _rx) = broadcast::channel(512);
            tx
        });
        sender.subscribe()
    }

    /// Publishes an event to any active subscribers for that resume_id
    pub async fn publish(&self, event: &CanvasEvent) {
        let senders = self.senders.read().await;
        if let Some(sender) = senders.get(&event.resume_id) {
            let _ = sender.send(event.clone());
        }
    }

    /// Retrieves or initializes the atomic sequence counter for a resume_id from SQLite.
    pub async fn get_or_init_counter(
        &self,
        pool: &SqlitePool,
        resume_id: i64,
    ) -> Result<Arc<AtomicU64>, sqlx::Error> {
        {
            let counters = self.counters.read().await;
            if let Some(counter) = counters.get(&resume_id) {
                return Ok(counter.clone());
            }
        }

        let mut counters = self.counters.write().await;
        if let Some(counter) = counters.get(&resume_id) {
            return Ok(counter.clone());
        }

        let max_seq_row: (i64,) = sqlx::query_as(
            "SELECT COALESCE(MAX(seq), 0) FROM generation_events WHERE resume_id = ?",
        )
        .bind(resume_id)
        .fetch_one(pool)
        .await?;

        let counter = Arc::new(AtomicU64::new(max_seq_row.0 as u64));
        counters.insert(resume_id, counter.clone());
        Ok(counter)
    }

    /// Increments the atomic counter and returns the next sequence number.
    pub async fn next_seq(&self, pool: &SqlitePool, resume_id: i64) -> Result<i64, sqlx::Error> {
        let counter = self.get_or_init_counter(pool, resume_id).await?;
        let next = counter.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(next as i64)
    }
}

/// Atomically persists an event to the generation_events table in SQLite
/// using the in-memory AtomicU64 sequence counter, and broadcasts it across
/// the event bus to all connected live subscribers.
pub async fn emit_event(
    pool: &SqlitePool,
    bus: &EventBus,
    resume_id: i64,
    node: &str,
    event_type: &str,
    payload: serde_json::Value,
) -> Result<CanvasEvent, sqlx::Error> {
    let seq = bus.next_seq(pool, resume_id).await?;
    let payload_str = payload.to_string();

    let row = sqlx::query_as::<_, GenerationEventRow>(
        "INSERT INTO generation_events (resume_id, seq, ts, node, event_type, payload_json) \
         VALUES (?, ?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), ?, ?, ?) \
         RETURNING id, resume_id, seq, ts, node, event_type, payload_json",
    )
    .bind(resume_id)
    .bind(seq)
    .bind(node)
    .bind(event_type)
    .bind(&payload_str)
    .fetch_one(pool)
    .await?;

    let event = CanvasEvent {
        resume_id: row.resume_id,
        node: row.node,
        seq: row.seq,
        ts: row.ts,
        event_type: row.event_type,
        payload,
    };

    bus.publish(&event).await;

    Ok(event)
}

/// Replays all persisted events for a resume_id ordered by seq ASC (for WebSocket catchup / REST fallback)
pub async fn get_events_for_resume(
    pool: &SqlitePool,
    resume_id: i64,
) -> Result<Vec<CanvasEvent>, sqlx::Error> {
    let rows = sqlx::query_as::<_, GenerationEventRow>(
        "SELECT id, resume_id, seq, ts, node, event_type, payload_json \
         FROM generation_events \
         WHERE resume_id = ? \
         ORDER BY seq ASC",
    )
    .bind(resume_id)
    .fetch_all(pool)
    .await?;

    let events = rows
        .into_iter()
        .map(|r| {
            let payload: serde_json::Value =
                serde_json::from_str(&r.payload_json).unwrap_or(serde_json::Value::Null);
            CanvasEvent {
                resume_id: r.resume_id,
                node: r.node,
                seq: r.seq,
                ts: r.ts,
                event_type: r.event_type,
                payload,
            }
        })
        .collect();

    Ok(events)
}

/// Maps a single raw Antigravity stream-json event object to Section 7 Canvas Event type & payload.
pub fn map_agy_event_to_canvas(
    val: &serde_json::Value,
) -> Option<(&'static str, serde_json::Value)> {
    if let Some(event_type) = val.get("event").and_then(|e| e.as_str()) {
        match event_type {
            "init" => {
                let conv_id = val
                    .pointer("/init/conversation_id")
                    .and_then(|s| s.as_str())
                    .unwrap_or("");
                let tools_count = val
                    .pointer("/init/tools")
                    .and_then(|t| t.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);
                Some((
                    "command_started",
                    serde_json::json!({
                        "conversation_id": conv_id,
                        "tools_count": tools_count,
                    }),
                ))
            }
            "step_update" => {
                let state = val.pointer("/step_update/state").and_then(|s| s.as_str());
                if state == Some("DONE") {
                    let duration = val.pointer("/step_update/duration_seconds");
                    let usage = val.pointer("/step_update/usage");
                    Some((
                        "command_output",
                        serde_json::json!({
                            "turn_done": true,
                            "duration": duration,
                            "usage": usage,
                        }),
                    ))
                } else if let Some(delta) = val
                    .pointer("/step_update/text_delta")
                    .and_then(|d| d.as_str())
                {
                    Some((
                        "command_output",
                        serde_json::json!({
                            "chunk": delta,
                        }),
                    ))
                } else {
                    None
                }
            }
            "result" => {
                let status = val
                    .pointer("/result/status")
                    .and_then(|s| s.as_str())
                    .unwrap_or("");
                if status == "SUCCESS" {
                    let content = val
                        .pointer("/result/response")
                        .and_then(|s| s.as_str())
                        .unwrap_or("");
                    Some((
                        "artifact_produced",
                        serde_json::json!({
                            "kind": "json",
                            "content": content,
                        }),
                    ))
                } else {
                    None
                }
            }
            _ => None,
        }
    } else {
        None
    }
}

/// Coalescer for streaming text deltas from Antigravity.
/// Buffers consecutive text_delta chunks for the same node and flushes as a single
/// command_output event either when the buffer exceeds ~200 characters or after ~250ms,
/// preventing token-level noise from cluttering generation_events and WebSocket streams.
pub struct DeltaCoalescer {
    buffer: String,
    last_flush: std::time::Instant,
    max_chars: usize,
    max_duration: std::time::Duration,
}

impl Default for DeltaCoalescer {
    fn default() -> Self {
        Self::new()
    }
}

impl DeltaCoalescer {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            last_flush: std::time::Instant::now(),
            max_chars: 200,
            max_duration: std::time::Duration::from_millis(250),
        }
    }

    pub fn push(&mut self, delta: &str) -> Option<String> {
        self.buffer.push_str(delta);
        if self.buffer.len() >= self.max_chars || self.last_flush.elapsed() >= self.max_duration {
            self.flush()
        } else {
            None
        }
    }

    pub fn flush(&mut self) -> Option<String> {
        if self.buffer.is_empty() {
            None
        } else {
            self.last_flush = std::time::Instant::now();
            let chunk = std::mem::take(&mut self.buffer);
            Some(chunk)
        }
    }
}

/// Coalesces raw stream-json events from Antigravity into high-level canvas events.
/// Consecutive text_delta chunks are buffered and flushed as single command_output events
/// (either every ~250ms or when the buffer exceeds ~200 chars).
pub fn coalesce_agy_events(
    raw_events: &[serde_json::Value],
) -> Vec<(&'static str, serde_json::Value)> {
    let mut out = Vec::new();
    let mut coalescer = DeltaCoalescer::new();

    for val in raw_events {
        if let Some(event_type) = val.get("event").and_then(|e| e.as_str()) {
            match event_type {
                "init" => {
                    if let Some(chunk) = coalescer.flush() {
                        out.push(("command_output", serde_json::json!({ "chunk": chunk })));
                    }
                    let conv_id = val
                        .pointer("/init/conversation_id")
                        .and_then(|s| s.as_str())
                        .unwrap_or("");
                    let tools_count = val
                        .pointer("/init/tools")
                        .and_then(|t| t.as_array())
                        .map(|a| a.len())
                        .unwrap_or(0);
                    out.push((
                        "command_started",
                        serde_json::json!({
                            "conversation_id": conv_id,
                            "tools_count": tools_count,
                        }),
                    ));
                }
                "step_update" => {
                    let state = val.pointer("/step_update/state").and_then(|s| s.as_str());
                    if state == Some("DONE") {
                        if let Some(chunk) = coalescer.flush() {
                            out.push(("command_output", serde_json::json!({ "chunk": chunk })));
                        }
                        let duration = val.pointer("/step_update/duration_seconds");
                        let usage = val.pointer("/step_update/usage");
                        out.push((
                            "command_output",
                            serde_json::json!({
                                "turn_done": true,
                                "duration": duration,
                                "usage": usage,
                            }),
                        ));
                    } else if let Some(delta) = val
                        .pointer("/step_update/text_delta")
                        .and_then(|d| d.as_str())
                    {
                        if let Some(chunk) = coalescer.push(delta) {
                            out.push(("command_output", serde_json::json!({ "chunk": chunk })));
                        }
                    }
                }
                "result" => {
                    if let Some(chunk) = coalescer.flush() {
                        out.push(("command_output", serde_json::json!({ "chunk": chunk })));
                    }
                    let status = val
                        .pointer("/result/status")
                        .and_then(|s| s.as_str())
                        .unwrap_or("");
                    if status == "SUCCESS" {
                        let content = val
                            .pointer("/result/response")
                            .and_then(|s| s.as_str())
                            .unwrap_or("");
                        out.push((
                            "artifact_produced",
                            serde_json::json!({
                                "kind": "json",
                                "content": content,
                            }),
                        ));
                    }
                }
                _ => {}
            }
        }
    }

    if let Some(chunk) = coalescer.flush() {
        out.push(("command_output", serde_json::json!({ "chunk": chunk })));
    }

    out
}
