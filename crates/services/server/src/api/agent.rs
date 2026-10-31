use crate::AppState;
use axum::{extract::State, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Deserialize)]
pub struct AgentChatRequest {
    pub message: String,
    pub session_id: Option<String>,
    pub system_override: Option<String>,
    pub model_override: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AgentChatResponse {
    pub success: bool,
    pub reply: String,
    pub model: String,
    pub endpoint: String,
    pub matches_found: usize,
    pub error: Option<String>,
}

/// Pluggable Agent Chat API: Supports any model configured via .env
pub async fn agent_chat_handler(
    State(state): State<AppState>,
    Json(payload): Json<AgentChatRequest>,
) -> impl IntoResponse {
    let cfg = &state.config;
    let model = payload
        .model_override
        .unwrap_or_else(|| cfg.llm_model.clone());
    let sys_prompt = payload
        .system_override
        .unwrap_or_else(|| cfg.agent_system_prompt.clone());

    // Retrieve context dynamically from RAG
    let knowledge = db_try!(state.rag.search_knowledge(&payload.message));
    let listings = db_try!(state.rag.search(&payload.message));

    // Retrieval evidence takes precedence over the lexical out-of-domain heuristic. This lets
    // domain questions through even when their wording contains a term the heuristic dislikes.
    let has_relevant_context = knowledge.iter().any(|result| result.relevance_score > 0.0)
        || listings.iter().any(|result| result.relevance_score > 1.0);
    if !has_relevant_context
        && agent::cache::AgentContextReasoner::is_unrelated_query(&payload.message)
    {
        return Json(AgentChatResponse {
            success: true,
            reply: cfg.unrelated_query_message.clone(),
            model,
            endpoint: cfg.llm_endpoint_url.clone(),
            matches_found: 0,
            error: None,
        }).into_response();
    }

    // If there is an active matching asset and the user asks an edge question (e.g. is it available or not, rent, rooms)
    if let Some(top_listing) = listings.first() {
        if let Some(cached_answer) = agent::cache::AgentContextReasoner::answer_from_cached_context(
            &top_listing.listing,
            &payload.message,
        ) {
            return Json(AgentChatResponse {
                success: true,
                reply: cached_answer,
                model,
                endpoint: cfg.llm_endpoint_url.clone(),
                matches_found: listings.len(),
                error: None,
            }).into_response();
        }
    }

    let mut context_summary = String::new();
    for result in knowledge.iter().take(5) {
        let content: String = result.document.content.chars().take(1800).collect();
        context_summary.push_str(&format!(
            "\nRelevant Knowledge [{} | {}]: {}\n",
            result.document.title, result.document.category, content
        ));
    }
    for result in listings.iter().take(5) {
        let listing = &result.listing;
        context_summary.push_str(&format!(
            "\nMatching Property: {} ({}) in {}, {} - Type: {} - Price/Rent: {} {} - Bedrooms: {} - Bathrooms: {} - Area: {} sqft - Description: {}\n",
            listing.title,
            listing.id,
            listing.location,
            listing.city,
            listing.listing_type,
            listing.price_aed,
            cfg.app_currency,
            listing.bedrooms,
            listing.bathrooms,
            listing.area_sqft,
            listing.description.chars().take(700).collect::<String>()
        ));
    }

    let combined_system = cfg
        .agent_context_prompt_template
        .replace("__SYSTEM_PROMPT__", &sys_prompt)
        .replace("__DOMAIN__", &cfg.app_domain_name)
        .replace("__CURRENCY__", &cfg.app_currency)
        .replace("__CONTEXT__", &context_summary);

    // Call configured pluggable LLM endpoint
    if !cfg.llm_endpoint_url.is_empty() && !cfg.llm_api_key.is_empty() {
        let req_body = json!({
            "model": model,
            "messages": [
                { "role": "system", "content": combined_system },
                { "role": "user", "content": payload.message }
            ],
            "temperature": 0.3,
            "max_tokens": 512
        });

        let client = reqwest::Client::new();
        let post_res = client
            .post(&cfg.llm_endpoint_url)
            .bearer_auth(&cfg.llm_api_key)
            .json(&req_body)
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await;

        if let Ok(resp) = post_res {
            if resp.status().is_success() {
                if let Ok(resp_json) = resp.json::<serde_json::Value>().await {
                    if let Some(reply_text) = resp_json
                        .pointer("/choices/0/message/content")
                        .and_then(|v| v.as_str())
                    {
                        return Json(AgentChatResponse {
                            success: true,
                            reply: reply_text.trim().to_string(),
                            model,
                            endpoint: cfg.llm_endpoint_url.clone(),
                            matches_found: knowledge.len() + listings.len(),
                            error: None,
                        }).into_response();
                    }
                }
            }
        }
    }

    // Fallback if endpoint is unreachable
    let fallback_reply = if let Some(top_doc) = knowledge.first() {
        top_doc.document.content.clone()
    } else if let Some(top_l) = listings.first() {
        cfg.property_match_message_template
            .replace("{title}", &top_l.listing.title)
            .replace("{location}", &top_l.listing.location)
            .replace("{price}", &top_l.listing.price_aed.to_string())
            .replace("{currency}", &cfg.app_currency)
    } else {
        cfg.domain_fallback_message.clone()
    };

    Json(AgentChatResponse {
        success: true,
        reply: fallback_reply,
        model,
        endpoint: cfg.llm_endpoint_url.clone(),
        matches_found: knowledge.len() + listings.len(),
        error: None,
    }).into_response()
}

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
    })).into_response()
}

/// Dynamically verifies whether all configured models (LLM, decision, STT, TTS) exist and respond
pub async fn agent_verify_models_handler(State(state): State<AppState>) -> impl IntoResponse {
    let report = agent::ModelChecker::verify_all_system_models(&state.config).await;
    Json(serde_json::to_value(report).unwrap_or_else(|_| json!({ "all_models_valid": false }))).into_response()
}
