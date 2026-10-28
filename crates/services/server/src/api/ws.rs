use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::IntoResponse,
};
use crate::AppState;
use orchestrator::QueueEvent;
use serde::Deserialize;

pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_websocket(socket, state))
}

/// Every message from the client must be JSON with at least a `session_id` and `type` field.
/// Example payloads:
///   { "session_id": "call-abc", "type": "STT_TRANSCRIPT", "text": "I want a villa" }
///   { "session_id": "call-abc", "type": "INTERVIEWER_SPEAKING_START" }
///   { "session_id": "call-abc", "type": "INTERVIEWER_SPEAKING_STOP" }
///   { "session_id": "call-abc", "type": "BARGE_IN" }
#[derive(Deserialize)]
struct WsClientMessage {
    session_id: String,
    #[serde(rename = "type")]
    msg_type: String,
    /// Optional: transcript text for STT_TRANSCRIPT messages
    text: Option<String>,
    /// Optional: language code, defaults to "en"
    language: Option<String>,
}

async fn handle_websocket(mut socket: WebSocket, state: AppState) {
    while let Some(Ok(msg)) = socket.recv().await {
        if let Message::Text(text) = msg {
            // Try to parse the structured JSON message first
            match serde_json::from_str::<WsClientMessage>(&text) {
                Ok(client_msg) => {
                    if client_msg.session_id.starts_with("phone:") { continue; }
                    let session_id = client_msg.session_id.clone();
                    let lang = client_msg.language.unwrap_or_else(|| "en".to_string());

                    let event = match client_msg.msg_type.as_str() {
                        "INTERVIEWER_SPEAKING_START" => Some(QueueEvent::InterviewerSpeech {
                            session_id,
                            interviewer_id: "interviewer-1".to_string(),
                            active: true,
                        }),
                        "INTERVIEWER_SPEAKING_STOP" => Some(QueueEvent::InterviewerSpeech {
                            session_id,
                            interviewer_id: "interviewer-1".to_string(),
                            active: false,
                        }),
                        "BARGE_IN" => Some(QueueEvent::BargeInTrigger {
                            session_id,
                            timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
                        }),
                        "STT_TRANSCRIPT" => {
                            if let Some(transcript_text) = client_msg.text {
                                Some(QueueEvent::SttTranscript {
                                    session_id,
                                    text: transcript_text,
                                    language: lang,
                                    is_final: true,
                                })
                            } else {
                                eprintln!("[WS] STT_TRANSCRIPT message missing 'text' field for session {}", client_msg.session_id);
                                None
                            }
                        }
                        unknown => {
                            eprintln!("[WS] Unknown message type '{}' from session {}", unknown, client_msg.session_id);
                            None
                        }
                    };

                    if let Some(ev) = event {
                        let _ = state.orch_tx.send(ev).await;
                    }
                }
                Err(_) => {
                    // Legacy fallback: plain-text keyword messages (kept for backwards compatibility)
                    // These will be dropped by the dispatcher since session_id is unknown.
                    // Clients should migrate to JSON format.
                    eprintln!(
                        "[WS] Received non-JSON message. Ignoring. Clients must send JSON with a 'session_id' field. Raw: {}",
                        &text[..text.len().min(120)]
                    );
                }
            }
        }
    }
}
