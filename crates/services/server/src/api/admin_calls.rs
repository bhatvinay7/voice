//! Paginated, searchable call-summary listing for the admin console.

use std::collections::HashMap;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use db::{CallEndpointsRecord, CallSummaryRecord, UserProfile};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::api::AdminUser;
use crate::AppState;

/// Upper bound protecting the server from oversized pages (validation, not config).
const MAX_PAGE_SIZE: usize = 100;
const FALLBACK_PAGE_SIZE: usize = 10;

#[derive(Debug, Deserialize)]
pub struct CallSummaryQuery {
    pub page: Option<usize>,
    pub page_size: Option<usize>,
    pub q: Option<String>,
}

struct PageWindow {
    page: usize,
    page_size: usize,
}

impl PageWindow {
    fn from_query(query: &CallSummaryQuery) -> Self {
        let page_size = query
            .page_size
            .unwrap_or(FALLBACK_PAGE_SIZE)
            .clamp(1, MAX_PAGE_SIZE);
        let page = query.page.unwrap_or(1).max(1);
        Self { page, page_size }
    }

    fn total_pages(&self, total: usize) -> usize {
        total.div_ceil(self.page_size).max(1)
    }

    fn offset(&self) -> usize {
        (self.page - 1) * self.page_size
    }
}

fn unavailable(error: impl ToString) -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({ "success": false, "error": error.to_string() })),
    )
        .into_response()
}

fn matches_search(
    record: &CallEndpointsRecord,
    summary: Option<&CallSummaryRecord>,
    user: Option<&UserProfile>,
    needle: &str,
) -> bool {
    if needle.is_empty() {
        return true;
    }
    let haystack = [
        Some(record.session_id.as_str()),
        user.map(|u| u.name.as_str()),
        user.map(|u| u.email.as_str()),
        summary.map(|s| s.summary.as_str()),
        summary.map(|s| s.preferred_location.as_str()),
    ];
    haystack
        .into_iter()
        .flatten()
        .any(|value| value.to_lowercase().contains(needle))
}

fn to_json(
    state: &AppState,
    record: &CallEndpointsRecord,
    summary: Option<&CallSummaryRecord>,
    user: Option<&UserProfile>,
) -> Value {
    json!({
        "call_id": record.session_id,
        "text_presigned_url": record.text_url,
        "audio_presigned_url": orchestrator::r2_storage::presign_conversation_url(&state.config, &record.session_id).ok(),
        "summary": summary.map(|s| s.summary.as_str()),
        "user_id": summary.map(|s| s.user_id.as_str()),
        "user_name": user.map(|u| u.name.as_str()),
        "user_email": user.map(|u| u.email.as_str()),
        "duration_seconds": summary.map(|s| s.duration_seconds),
        "qualified_budget_aed": summary.map(|s| s.qualified_budget_aed),
        "preferred_location": summary.map(|s| s.preferred_location.as_str()),
        "created_at": record.created_at,
    })
}

pub async fn admin_get_call_summaries(
    _admin: AdminUser,
    State(state): State<AppState>,
    Query(query): Query<CallSummaryQuery>,
) -> Response {
    let recordings = match state.db.load_call_endpoints().await {
        Ok(recordings) => recordings,
        Err(error) => return unavailable(error),
    };
    let summaries = match state.db.load_call_summaries().await {
        Ok(summaries) => summaries,
        Err(error) => return unavailable(error),
    };
    let users = match state.db.get_all_users().await {
        Ok(users) => users,
        Err(error) => return unavailable(error),
    };

    let summary_by_id: HashMap<&str, &CallSummaryRecord> =
        summaries.iter().map(|s| (s.call_id.as_str(), s)).collect();
    let user_by_id: HashMap<&str, &UserProfile> =
        users.iter().map(|u| (u.id.as_str(), u)).collect();
    let needle = query.q.as_deref().unwrap_or("").trim().to_lowercase();

    let filtered: Vec<(
        &CallEndpointsRecord,
        Option<&CallSummaryRecord>,
        Option<&UserProfile>,
    )> = recordings
        .iter()
        .filter(|record| !record.session_id.starts_with("phone:"))
        .map(|record| {
            let summary = summary_by_id.get(record.session_id.as_str()).copied();
            let user = summary.and_then(|s| user_by_id.get(s.user_id.as_str()).copied());
            (record, summary, user)
        })
        .filter(|(record, summary, user)| matches_search(record, *summary, *user, &needle))
        .collect();

    let window = PageWindow::from_query(&query);
    let total = filtered.len();
    // Presign only the visible page; presigning every recording does not scale.
    let page_items: Vec<Value> = filtered
        .iter()
        .skip(window.offset())
        .take(window.page_size)
        .map(|(record, summary, user)| to_json(&state, record, *summary, *user))
        .collect();

    Json(json!({
        "success": true,
        "summaries": page_items,
        "pagination": {
            "page": window.page,
            "page_size": window.page_size,
            "total": total,
            "total_pages": window.total_pages(total),
        },
    }))
    .into_response()
}
