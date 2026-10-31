macro_rules! db_try {
    ($operation:expr) => {
        match $operation.await {
            Ok(value) => value,
            Err(_) => return axum::response::IntoResponse::into_response((
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                axum::Json(serde_json::json!({"success": false, "error": "Database unavailable"})),
            )),
        }
    };
}

use axum::{
    response::IntoResponse,
    routing::{get, post, put},
    Json, Router,
};
use serde_json::json;

use crate::AppState;

pub mod auth;
pub mod listings;
pub mod admin;
pub mod contracts;
pub mod outbox;
pub mod calls;
pub mod ws;
pub mod rbac;
pub mod agent;
pub mod telephony;

#[allow(unused_imports)]
pub use rbac::{AdminUser, AuthUser};

pub fn api_routes() -> Router<AppState> {
    // Admin management routes protected by JWT middleware checking role == "admin"
    let admin_routes = Router::new()
        .route("/api/rag/upsert", post(admin::admin_add_listing))
        .route("/api/rag/knowledge/upsert", post(admin::admin_add_knowledge_doc))
        .route("/api/admin/knowledge", get(admin::admin_get_knowledge_docs).post(admin::admin_add_knowledge_doc))
        .route("/api/admin/listings", post(admin::admin_add_listing))
        .route(
            "/api/admin/listings/{id}",
            put(admin::admin_update_listing).delete(admin::admin_delete_listing),
        )
        .route("/api/admin/graph", get(admin::admin_get_graph))
        .route("/api/admin/graph/nodes", post(admin::admin_add_graph_node))
        .route(
            "/api/admin/graph/nodes/{id}",
            put(admin::admin_update_graph_node).delete(admin::admin_delete_graph_node),
        )
        .route("/api/admin/logs", get(admin::admin_get_logs))
        .route("/api/admin/call-summaries", get(admin::admin_get_call_summaries))
        .route("/api/admin/sessions/{id}", get(admin::admin_get_session_details))
        .route("/api/contracts/review-queue", get(contracts::get_review_queue))
        .route("/api/contracts/approve", post(contracts::approve_contract))
        .route("/api/outbox/pending", get(outbox::get_pending_outbox))
        .route("/api/outbox/process", post(outbox::process_outbox))
        .layer(axum::middleware::from_fn(rbac::require_admin_role_middleware));

    Router::new()
        .route("/health", get(health_check))
        
        // Agent Endpoints (Pluggable Model & Config)
        .route("/api/agent/chat", post(agent::agent_chat_handler))
        .route("/api/agent/config", get(agent::agent_get_config_handler))
        .route("/api/agent/models/check", get(agent::agent_verify_models_handler))

        // Phone-provider callbacks are isolated from browser WebRTC routes.
        .route("/api/telephony/status", get(telephony::status_handler))
        .route("/api/telephony/{provider}/incoming", post(telephony::inbound_handler))
        .route("/api/telephony/{provider}/media/{call_id}", get(telephony::media_handler))
        .route("/api/telephony/{provider}/media/{call_id}/{token}", get(telephony::media_handler))

        // Listings & Search
        .route("/api/listings", get(listings::get_listings))
        .route("/api/listings/search", post(listings::search_listings))
        .route("/api/rag/search", post(listings::search_listings))
        .route("/api/rag/knowledge", get(listings::get_knowledge_documents))
        
        // Bookings
        .route("/api/bookings", post(listings::book_slot))
        
        // Users & Auth
        .route("/api/users/verify", post(auth::verify_user))
        .route("/api/auth/login", post(auth::auth_email))
        .route("/api/auth/google", post(auth::auth_google))
        .route("/api/auth/me", get(auth::auth_me))
        .route("/api/auth/logout", post(auth::auth_logout))
        
        // Admin Login
        .route("/api/admin/login", post(admin::admin_login))
        
        // Calls & Suggestions
        .route("/api/calls/session", post(calls::init_call_session))
        .route("/api/calls/end", post(calls::end_call_session))
        .route("/api/calls/pause", post(calls::pause_call_handler))
        .route("/api/calls/suggestions/{session_id}", get(calls::get_cached_suggestions))
        .route("/api/calls/suggestions/sse", get(calls::sse_suggestions_handler))
        
        // Contracts Upload
        .route("/api/contracts/upload", post(contracts::upload_contract))
        
        // WebSocket
        .route("/ws", get(ws::ws_handler))
        
        // Mount protected admin routes
        .merge(admin_routes)
}

async fn health_check() -> impl IntoResponse {
    Json(json!({
        "status": "ok",
        "service": "uae-real-estate-backend-rust",
        "grpc_enabled": true,
        "sse_suggestions_enabled": true,
        "redis_streams_sync": true,
        "rag_vector_db_enabled": true,
        "outbox_worker": true,
        "timestamp": chrono::Utc::now().to_rfc3339()
    }))
}
