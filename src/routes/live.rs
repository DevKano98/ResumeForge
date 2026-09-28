use crate::services::event_bus;
use crate::state::AppState;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path as AxumPath, State,
    },
    response::IntoResponse,
};

/// GET /ws/resumes/:id/live
/// Establishes a WebSocket connection that immediately replays all past generation events
/// from SQLite, then streams live events broadcast across the event bus.
pub async fn ws_resume_live(
    ws: WebSocketUpgrade,
    AxumPath(id): AxumPath<i64>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_resume_ws(socket, id, state))
}

async fn handle_resume_ws(mut socket: WebSocket, resume_id: i64, state: AppState) {
    // 1. Subscribe to the live broadcast channel FIRST to ensure no gap between DB and live
    let mut rx = state.event_bus.subscribe(resume_id).await;

    // 2. Fetch and replay all existing events from SQLite in seq order
    let mut last_seq = 0i64;
    match event_bus::get_events_for_resume(&state.db, resume_id).await {
        Ok(events) => {
            for event in events {
                if event.seq > last_seq {
                    last_seq = event.seq;
                }
                if let Ok(text) = serde_json::to_string(&event) {
                    if socket.send(Message::Text(text.into())).await.is_err() {
                        return;
                    }
                }
            }
        }
        Err(err) => {
            tracing::error!(
                "Failed to fetch historical events for resume {}: {}",
                resume_id,
                err
            );
        }
    }

    // 3. Live stream loop: forward new broadcast events to the WebSocket client
    loop {
        tokio::select! {
            recv_result = rx.recv() => {
                match recv_result {
                    Ok(event) => {
                        // Deduplicate any event that was already sent during replay
                        if event.seq > last_seq {
                            last_seq = event.seq;
                            if let Ok(text) = serde_json::to_string(&event) {
                                if socket.send(Message::Text(text.into())).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(missed)) => {
                        tracing::warn!("WS subscriber for resume {} lagged by {} events; catching up from DB", resume_id, missed);
                        if let Ok(catchup) = event_bus::get_events_for_resume(&state.db, resume_id).await {
                            for ev in catchup {
                                if ev.seq > last_seq {
                                    last_seq = ev.seq;
                                    if let Ok(text) = serde_json::to_string(&ev) {
                                        if socket.send(Message::Text(text.into())).await.is_err() {
                                            return;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
            client_msg = socket.recv() => {
                match client_msg {
                    Some(Ok(Message::Close(_))) | None => {
                        break;
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        if socket.send(Message::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}
