use axum::{extract::State, response::IntoResponse, Json};
use serde::Deserialize;
use serde_json::json;
use crate::AppState;
use crate::api::rbac::create_jwt_token;
use crate::api::AdminUser;
use db::{AdminGraphNode, KnowledgeDocument, PropertyListing};

#[derive(Deserialize)]
#[allow(dead_code)]
pub struct AdminLoginReq {
    pub email: String,
    pub password: String,
}

pub(crate) fn valid_admin_credentials(cfg: &config::AppConfig, email: &str, password: &str) -> bool {
    use hmac::{Hmac, Mac};
    if cfg.admin_email.is_empty() || cfg.admin_password.is_empty() || !email.eq_ignore_ascii_case(&cfg.admin_email) {
        return false;
    }
    let mut expected = Hmac::<sha2::Sha256>::new_from_slice(cfg.admin_password.as_bytes()).expect("HMAC accepts all key sizes");
    let mut supplied = Hmac::<sha2::Sha256>::new_from_slice(password.as_bytes()).expect("HMAC accepts all key sizes");
    expected.update(b"admin-login");
    supplied.update(b"admin-login");
    expected.verify_slice(&supplied.finalize().into_bytes()).is_ok()
}

pub async fn admin_login(
    State(state): State<AppState>,
    Json(payload): Json<AdminLoginReq>,
) -> impl IntoResponse {
    if state.config.admin_email.is_empty() || state.config.admin_password.is_empty() {
        return (axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"success": false, "error": "Set ADMIN_EMAIL and ADMIN_PASSWORD in the server environment"}))).into_response();
    }
    if !valid_admin_credentials(&state.config, &payload.email, &payload.password) {
        return (axum::http::StatusCode::UNAUTHORIZED,
            Json(json!({"success": false, "error": "Invalid administrator credentials"}))).into_response();
    }
    let user = match db_try!(state.db.get_user_by_email(&state.config.admin_email)) {
        Some(user) => user,
        None => {
            let id = format!("user-{}", state.config.admin_email.replace('@', "-").replace('.', "-"));
            db_try!(state.db.upsert_user(&id, "", &state.config.admin_email, "admin", "email"))
        }
    };

    // 2. Check the role from the database - no hardcoded credentials!
    if user.role != "admin" {
        return (
            axum::http::StatusCode::FORBIDDEN,
            Json(json!({
                "success": false,
                "error": "Forbidden: User does not have administrator role in database"
            })),
        )
            .into_response();
    }

    // 3. Issue cryptographically signed JWT containing role: "admin"
    let token = match create_jwt_token(&user.id, &user.email, &user.role) {
        Ok(t) => t,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "success": false,
                    "error": e
                })),
            )
                .into_response();
        }
    };

    (
        axum::http::StatusCode::OK,
        Json(json!({
            "success": true,
            "token": token,
            "user": user
        })),
    )
        .into_response()
}

pub async fn admin_add_listing(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(payload): Json<PropertyListing>,
) -> impl IntoResponse {
    db_try!(state.db.add_listing(payload.clone()));

    if let Err(e) = state.rag.upsert_property_to_vector(&payload).await {
        eprintln!("Failed to upsert to Upstash Vector: {}", e);
    }

    Json(json!({ "success": true, "listing": payload })).into_response()
}

pub async fn admin_update_listing(
    _admin: AdminUser,
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(payload): Json<PropertyListing>,
) -> impl IntoResponse {
    let updated = db_try!(state.db.update_listing(&id, payload.clone()));

    if updated {
        if let Err(e) = state.rag.upsert_property_to_vector(&payload).await {
            eprintln!("Failed to upsert to Upstash Vector: {}", e);
        }
    }

    Json(json!({ "success": updated, "listing": payload })).into_response()
}

pub async fn admin_delete_listing(
    _admin: AdminUser,
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> impl IntoResponse {
    let deleted = db_try!(state.db.delete_listing(&id));
    Json(json!({ "success": deleted })).into_response()
}

pub async fn admin_get_graph(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let nodes = db_try!(state.db.get_graph_nodes());
    Json(json!({ "success": true, "graph_nodes": nodes })).into_response()
}

pub async fn admin_add_graph_node(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(payload): Json<AdminGraphNode>,
) -> impl IntoResponse {
    db_try!(state.db.add_graph_node(payload.clone()));
    Json(json!({ "success": true, "node": payload })).into_response()
}

#[derive(Deserialize)]
pub struct UpdateGraphNodeReq {
    pub instruction: String,
}

pub async fn admin_update_graph_node(
    _admin: AdminUser,
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(payload): Json<UpdateGraphNodeReq>,
) -> impl IntoResponse {
    let updated = db_try!(state.db.update_graph_node(&id, payload.instruction));
    Json(json!({ "success": updated })).into_response()
}

pub async fn admin_delete_graph_node(
    _admin: AdminUser,
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> impl IntoResponse {
    let deleted = db_try!(state.db.delete_graph_node(&id));
    Json(json!({ "success": deleted })).into_response()
}

pub async fn admin_get_logs(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> impl IntoResponse {
    match state.db.load_recent_stream_events().await {
        Ok(events) => {
            let events: Vec<_> = events.into_iter().filter(|event| !event.session_id.starts_with("phone:")).collect();
            Json(json!({ "success": true, "logs": events })).into_response()
        },
        Err(error) => (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "success": false, "error": error })),
        )
            .into_response(),
    }
}

pub async fn admin_get_call_summaries(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let recordings = match state.db.load_call_endpoints().await {
        Ok(recordings) => recordings,
        Err(error) => {
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "success": false, "error": error })),
            )
                .into_response();
        }
    };
    let summaries = match state.db.load_call_summaries().await {
        Ok(summaries) => summaries,
        Err(error) => {
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "success": false, "error": error })),
            )
                .into_response();
        }
    };
    let enriched: Vec<serde_json::Value> = recordings
        .iter()
        .filter(|record| !record.session_id.starts_with("phone:"))
        .map(|record| {
            let summary = summaries.iter().find(|summary| summary.call_id == record.session_id);
            let created_at_readable = chrono::DateTime::parse_from_rfc3339(&record.created_at)
                .map(|time| time.format("%d %b %Y, %H:%M UTC").to_string())
                .unwrap_or_else(|_| record.created_at.clone());
            json!({
                "call_id": record.session_id,
                "text_presigned_url": record.text_url,
                "audio_presigned_url": orchestrator::r2_storage::presign_conversation_url(&state.config, &record.session_id).ok(),
                "audio_filename": format!("{}.wav", record.session_id),
                "summary": summary.map(|summary| summary.summary.as_str()),
                "user_id": summary.map(|summary| summary.user_id.as_str()),
                "duration_seconds": summary.map(|summary| summary.duration_seconds),
                "qualified_budget_aed": summary.map(|summary| summary.qualified_budget_aed),
                "preferred_location": summary.map(|summary| summary.preferred_location.as_str()),
                "created_at": record.created_at,
                "created_at_readable": created_at_readable,
            })
        })
        .collect();
    Json(json!({ "success": true, "summaries": enriched })).into_response()
}


pub async fn admin_get_session_details(
    _admin: AdminUser,
    State(state): State<AppState>,
    axum::extract::Path(session_id): axum::extract::Path<String>,
) -> impl IntoResponse {
    if session_id.starts_with("phone:") { return super::calls::phone_route_denied(); }
    let summary = match state.db.load_call_summary(&session_id).await {
        Ok(summary) => summary,
        Err(error) => {
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "success": false, "error": error })),
            )
                .into_response();
        }
    };
    let user_profile = if let Some(ref s) = summary {
        db_try!(state.db.get_user(&s.user_id))
    } else {
        None
    };
    let all_events = match state.db.load_session_events(&session_id).await {
        Ok(events) => events,
        Err(error) => {
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "success": false, "error": error })),
            )
                .into_response();
        }
    };
    let session_events: Vec<_> = all_events
        .into_iter()
        .filter(|e| e.session_id == session_id || e.session_id == format!("run-{session_id}"))
        .collect();
    let conversation_events: Vec<_> = session_events
        .iter()
        .filter(|e| e.stream == "stream:conversation")
        .collect();
    let audit_events: Vec<_> = session_events
        .iter()
        .filter(|e| e.stream == "stream:audit")
        .collect();
    let logs_events: Vec<_> = session_events
        .iter()
        .filter(|e| e.stream == "stream:logs")
        .collect();

    let endpoints = state
        .db
        .load_call_endpoints()
        .await
        .map(|records| records.into_iter().find(|record| record.session_id == session_id));
    let endpoints = match endpoints {
        Ok(Some(mut record)) => {
            match orchestrator::r2_storage::presign_conversation_url(
                &state.config,
                &record.session_id,
            ) {
                Ok(url) => record.audio_url = url,
                Err(error) => {
                    return (
                        axum::http::StatusCode::SERVICE_UNAVAILABLE,
                        Json(json!({ "success": false, "error": error })),
                    )
                        .into_response();
                }
            }
            Some(record)
        }
        Ok(None) => None,
        Err(error) => {
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "success": false, "error": error })),
            )
                .into_response();
        }
    };

    // Enrich summary with readable timings and audio file metadata
    let enriched_summary = summary.as_ref().map(|s| {
        let mins = s.duration_seconds / 60;
        let secs = s.duration_seconds % 60;
        let duration_readable = if mins > 0 {
            format!("{}m {}s", mins, secs)
        } else {
            format!("{}s", secs)
        };
        let created_at_readable = chrono::DateTime::parse_from_rfc3339(&s.created_at)
            .map(|dt| dt.format("%d %b %Y, %H:%M UTC").to_string())
            .unwrap_or_else(|_| s.created_at.clone());
        let audio_filename = s.audio_presigned_url.split('/').last().unwrap_or("recording.wav").to_string();
        json!({
            "call_id": s.call_id,
            "user_id": s.user_id,
            "duration_seconds": s.duration_seconds,
            "duration_readable": duration_readable,
            "audio_presigned_url": s.audio_presigned_url,
            "audio_filename": audio_filename,
            "text_presigned_url": s.text_presigned_url,
            "summary": s.summary,
            "qualified_budget_aed": s.qualified_budget_aed,
            "preferred_location": s.preferred_location,
            "created_at": s.created_at,
            "created_at_readable": created_at_readable,
        })
    }).or_else(|| endpoints.as_ref().map(|record| {
        let created_at_readable = chrono::DateTime::parse_from_rfc3339(&record.created_at)
            .map(|time| time.format("%d %b %Y, %H:%M UTC").to_string())
            .unwrap_or_else(|_| record.created_at.clone());
        json!({
            "call_id": record.session_id,
            "duration_seconds": null,
            "audio_presigned_url": record.audio_url,
            "text_presigned_url": record.text_url,
            "summary": null,
            "qualified_budget_aed": null,
            "preferred_location": null,
            "created_at": record.created_at,
            "created_at_readable": created_at_readable,
        })
    }));

    Json(json!({
        "success": true,
        "session_id": session_id,
        "summary": enriched_summary,
        "user": user_profile,
        "endpoints": endpoints,
        "conversation": conversation_events,
        "audit": audit_events,
        "logs": logs_events,
        "raw_events": session_events
    }))
        .into_response()
}

pub async fn admin_add_knowledge_doc(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(payload): Json<KnowledgeDocument>,
) -> impl IntoResponse {
    db_try!(state.db.add_knowledge_document(payload.clone()));

    if let Err(e) = state.rag.upsert_document_to_vector(&payload).await {
        eprintln!("Failed to upsert knowledge document to Upstash Vector: {}", e);
    }

    Json(json!({ "success": true, "document": payload })).into_response()
}

pub async fn admin_get_knowledge_docs(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let docs = db_try!(state.db.get_knowledge_documents());
    Json(json!({ "success": true, "documents": docs })).into_response()
}

