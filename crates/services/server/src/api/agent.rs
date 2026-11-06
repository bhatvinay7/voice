use crate::AppState;
use axum::{extract::State, response::IntoResponse, Json};
use serde_json::json;

/// Returns currently active models and domain parameters from .env
pub async fn agent_get_config_handler(State(state): State<AppState>) -> impl IntoResponse {
    let cfg = &state.config;
    Json(json!({
        "domain": cfg.app_domain_name,
        "currency": cfg.app_currency,
        "llm_model": cfg.llm_model,
        "llm_endpoint": cfg.llm_endpoint_url,
        "decision_llm_model": cfg.decision_llm_model,
        "decision_llm_endpoint": cfg.decision_llm_endpoint_url,
        "stt_model": cfg.stt_model,
        "tts_model": cfg.tts_model,
        "tts_voice": cfg.tts_voice,
        "kyc_document_name": cfg.kyc_document_name,
        "kyc_required_for_booking": cfg.kyc_required_for_booking,
        "action_booking_keywords": cfg.action_booking_keywords,
        "filter_primary_type_a": cfg.filter_primary_type_a,
        "filter_primary_type_b": cfg.filter_primary_type_b,
    }))
    .into_response()
}

/// Dynamically verifies whether all configured models (LLM, decision, STT, TTS) exist and respond
pub async fn agent_verify_models_handler(State(state): State<AppState>) -> impl IntoResponse {
    let report = agent::ModelChecker::verify_all_system_models(&state.config).await;
    Json(serde_json::to_value(report).unwrap_or_else(|_| json!({ "all_models_valid": false })))
        .into_response()
}
