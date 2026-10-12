use axum::{extract::State, response::IntoResponse, Json};
use serde_json::json;
use crate::AppState;
use crate::api::AdminUser;

pub async fn get_pending_outbox(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let notifs = state.db.get_outbox_notifications();
    Json(json!({ "success": true, "outbox": notifs }))
}

pub async fn process_outbox(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let processed_count = state.db.process_outbox_notifications();
    Json(json!({ "success": true, "processed_count": processed_count }))
}
