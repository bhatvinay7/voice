use crate::AppState;
use axum::{
    extract::{Path, Query, State},
    response::{
        sse::{Event, Sse},
        IntoResponse,
    },
    Json,
};
use orchestrator::QueueEvent;
use serde::Deserialize;
use serde_json::json;
use std::convert::Infallible;
use tokio_stream::StreamExt;

#[derive(Deserialize)]
pub struct CallReq {
    pub call_id: String,
    pub user_id: String,
}

pub async fn init_call_session(
    State(state): State<AppState>,
    Json(payload): Json<CallReq>,
) -> impl IntoResponse {
    if payload.call_id.starts_with("phone:") { return phone_route_denied(); }
    let token = match livekit_api::access_token::AccessToken::with_api_key(
        &state.config.livekit_api_key,
        &state.config.livekit_api_secret,
    )
    .with_identity(&payload.user_id)
    .with_name(&payload.user_id)
    .with_grants(livekit_api::access_token::VideoGrants {
        room_join: true,
        room: payload.call_id.clone(),
        can_publish: Some(true),
        can_subscribe: Some(true),
        can_publish_data: Some(true),
        ..Default::default()
    })
    .to_jwt()
    {
        Ok(t) => t,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "success": false,
                    "error": format!("Failed to generate LiveKit JWT token: {}", e)
                })),
            )
                .into_response();
        }
    };

    let acquired = match state.slots.acquire(&payload.call_id, chrono::Utc::now().timestamp()) {
        Ok(acquired) => acquired,
        Err(error) => return (axum::http::StatusCode::SERVICE_UNAVAILABLE, Json(json!({"success": false, "error": error}))).into_response(),
    };
    if acquired && state.orch_tx.try_send(QueueEvent::SessionStarted {
        session_id: payload.call_id.clone(), user_id: payload.user_id.clone(), language: "en".into(),
    }).is_err() {
        let _ = state.slots.release(&payload.call_id);
        return (axum::http::StatusCode::SERVICE_UNAVAILABLE, Json(json!({"success": false, "error": "session queue unavailable"}))).into_response();
    }

    (
        axum::http::StatusCode::OK,
        Json(json!({
            "success": true,
            "call_id": payload.call_id,
            "user_id": payload.user_id,
            "livekit_token": token,
            "livekit_url": state.config.livekit_url
        })),
    )
        .into_response()
}

#[derive(Deserialize)]
pub struct SseQuery {
    pub call_id: Option<String>,
}

pub async fn get_cached_suggestions(
    Path(session_id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if session_id.starts_with("phone:") { return phone_route_denied(); }
    let cache_key = format!("session:{}:suggestions", session_id);
    match state.db.get_json(&cache_key).await {
        Ok(Some(json_str)) => {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&json_str) {
                return Json(val).into_response();
            }
        }
        _ => {}
    }
    Json(json!({ "success": false, "error": "No suggestions found or expired." })).into_response()
}

pub async fn sse_suggestions_handler(
    Query(query): Query<SseQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    // call_id is required — without it we cannot filter the right session's events.
    let call_id = match query.call_id {
        Some(id) if !id.is_empty() => id,
        _ => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(json!({ "success": false, "error": "call_id query param is required" })),
            )
                .into_response();
        }
    };
    if call_id.starts_with("phone:") { return phone_route_denied(); }
    let rx = state.pubsub.subscribe();
    let stream =
        tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(move |msg| match msg {
            Ok(pubsub::SystemEvent::AuditEvent {
                call_id: Some(cid),
                event_type,
                payload,
                ..
            }) => {
                if cid == call_id && event_type == "DEAL_SUGGESTIONS_GENERATED" {
                    Some(Ok::<axum::response::sse::Event, Infallible>(
                        Event::default().data(payload.to_string()),
                    ))
                } else {
                    None
                }
            }
            _ => None,
        });
    Sse::new(stream).into_response()
}

#[derive(Deserialize)]
pub struct PauseCallReq {
    pub session_id: String,
    pub paused: bool,
}

pub async fn pause_call_handler(
    State(state): State<AppState>,
    Json(payload): Json<PauseCallReq>,
) -> impl IntoResponse {
    if payload.session_id.starts_with("phone:") { return phone_route_denied(); }
    let _ = state
        .orch_tx
        .send(QueueEvent::InterviewerSpeech {
            session_id: payload.session_id.clone(),
            interviewer_id: "client-user".to_string(),
            active: payload.paused,
        })
        .await;

    Json(json!({ "success": true, "session_id": payload.session_id, "paused": payload.paused })).into_response()
}

pub async fn end_call_session(
    State(state): State<AppState>,
    Json(payload): Json<CallReq>,
) -> impl IntoResponse {
    if payload.call_id.starts_with("phone:") { return phone_route_denied(); }
    let _ = state
        .orch_tx
        .send(QueueEvent::SessionCancelled {
            session_id: payload.call_id.clone(),
        })
        .await;

    Json(json!({
        "success": true,
        "call_id": payload.call_id,
        "recording_status": "finalizing"
    })).into_response()
}

pub(super) fn phone_route_denied() -> axum::response::Response {
    (axum::http::StatusCode::NOT_FOUND, Json(json!({"success": false, "error": "Call is not available through the web API"}))).into_response()
}
