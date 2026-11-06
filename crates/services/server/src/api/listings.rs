use crate::AppState;
use axum::{extract::State, response::IntoResponse, Json};
use orchestrator::QueueEvent;
use serde::Deserialize;
use serde_json::json;

pub async fn get_listings(State(state): State<AppState>) -> impl IntoResponse {
    let listings = db_try!(state.db.get_listings());
    Json(json!({ "success": true, "listings": listings })).into_response()
}

#[derive(Deserialize)]
pub struct SearchReq {
    #[serde(default)]
    pub query: String,
    /// Required for assistant and agent requests so work is owned by one
    /// independently cancellable session actor.
    pub session_id: Option<String>,
    pub location: Option<String>,
    pub max_price_aed: Option<u64>,
    pub bedrooms: Option<u8>,
    pub listing_type: Option<String>,
}

pub async fn search_listings(
    State(state): State<AppState>,
    Json(payload): Json<SearchReq>,
) -> impl IntoResponse {
    if payload.session_id.is_some() {
        let session_id = match payload
            .session_id
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            Some(value) => value,
            None => {
                return (
                    axum::http::StatusCode::BAD_REQUEST,
                    Json(json!({"success": false, "error": "session_id must not be empty"})),
                )
                    .into_response()
            }
        };
        if state
            .orch_tx
            .send(QueueEvent::SttTranscript {
                session_id: session_id.to_string(),
                text: payload.query.clone(),
                language: "en".to_string(),
                is_final: true,
                noise: None,
            })
            .await
            .is_err()
        {
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"success": false, "error": "session queue unavailable"})),
            )
                .into_response();
        }
        return Json(json!({
            "success": true,
            "status": "accepted",
            "session_id": session_id,
        }))
        .into_response();
    }

    let location = payload
        .location
        .as_deref()
        .unwrap_or(&payload.query)
        .trim()
        .to_lowercase();
    let listing_type = payload.listing_type.as_deref().map(str::to_lowercase);
    let listings = db_try!(state.db.get_listings())
        .into_iter()
        .filter(|listing| {
            let location_matches = location.is_empty()
                || listing.location.to_lowercase().contains(&location)
                || listing.city.to_lowercase().contains(&location)
                || listing.title.to_lowercase().contains(&location);
            let price_matches = payload
                .max_price_aed
                .map(|price| listing.price_aed <= price)
                .unwrap_or(true);
            let bedrooms_match = payload
                .bedrooms
                .map(|beds| listing.bedrooms == beds)
                .unwrap_or(true);
            let type_matches = listing_type
                .as_ref()
                .map(|kind| listing.listing_type.eq_ignore_ascii_case(kind))
                .unwrap_or(true);
            location_matches && price_matches && bedrooms_match && type_matches
        })
        .collect::<Vec<_>>();

    Json(json!({
        "success": true,
        "listings": listings,
    }))
    .into_response()
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
