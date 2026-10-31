use axum::{extract::State, response::IntoResponse, Json};
use serde::Deserialize;
use serde_json::json;
use crate::AppState;
use orchestrator::QueueEvent;

pub async fn get_listings(State(state): State<AppState>) -> impl IntoResponse {
    let listings = db_try!(state.db.get_listings());
    Json(json!({ "success": true, "listings": listings })).into_response()
}

#[derive(Deserialize)]
pub struct SearchReq {
    pub query: String,
    /// Optional active call/session ID. If provided, the query is dispatched
    /// to the orchestrator for session context tracking.
    pub session_id: Option<String>,
}

pub async fn search_listings(
    State(state): State<AppState>,
    Json(payload): Json<SearchReq>,
) -> impl IntoResponse {
    if let Some(session_id) = &payload.session_id {
        let _ = state
            .orch_tx
            .send(QueueEvent::SttTranscript {
                session_id: session_id.clone(),
                text: payload.query.clone(),
                language: "en".to_string(),
                is_final: true,
            })
            .await;
    }

    let rag_results = db_try!(state.rag.search(&payload.query));
    let knowledge_results = db_try!(state.rag.search_knowledge(&payload.query));

    Json(json!({ 
        "success": true, 
        "message": "Query processed via RAG vector search pipeline.",
        "results": rag_results,
        "knowledge_results": knowledge_results
    })).into_response()
}

pub async fn get_knowledge_documents(State(state): State<AppState>) -> impl IntoResponse {
    let docs = db_try!(state.db.get_knowledge_documents());
    Json(json!({ "success": true, "documents": docs })).into_response()
}

#[derive(Deserialize)]
pub struct BookingReq {
    pub listing_id: String,
    pub user_id: String,
    pub slot_time: String,
    pub idempotency_key: String,
    /// Optional active call/session ID for audit trail correlation.
    pub session_id: Option<String>,
}

pub async fn book_slot(
    State(state): State<AppState>,
    Json(payload): Json<BookingReq>,
) -> impl IntoResponse {
    let res = db_try!(state.db.book_viewing_slot(
        &payload.listing_id,
        &payload.user_id,
        &payload.slot_time,
        &payload.idempotency_key,
    ));

    let session_id = payload.session_id.as_deref().unwrap_or("web-portal");
    let trace_id = format!("trace-{}", chrono::Utc::now().timestamp_millis());
    let _ = state
        .db
        .push_redis_stream(
            "stream:audit",
            session_id,
            &trace_id,
            "SLOT_BOOKING_REQUEST",
            &json!({ "listing_id": payload.listing_id, "user_id": payload.user_id }).to_string(),
        )
        .await;
    Json(serde_json::to_value(res).unwrap_or(json!({"success": false}))).into_response()
}
