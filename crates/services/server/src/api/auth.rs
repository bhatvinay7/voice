use axum::{extract::State, response::IntoResponse, Json};
use serde::Deserialize;
use serde_json::json;
use crate::AppState;
use crate::api::rbac::{create_jwt_token, verify_jwt_token};

#[derive(Deserialize)]
pub struct VerifyReq {
    pub user_id: String,
    pub emirates_id_number: String,
}

pub async fn verify_user(
    State(state): State<AppState>,
    Json(payload): Json<VerifyReq>,
) -> impl IntoResponse {
    let updated = db_try!(state
        .db
        .verify_user_document(&payload.user_id, &payload.emirates_id_number));
    let trace_id = format!("trace-{}", chrono::Utc::now().timestamp_millis());
    let _ = state
        .db
        .push_redis_stream(
            "stream:audit",
            "system",   // auth events are not scoped to a call session
            &trace_id,
            "USER_VERIFIED",
            &json!({ "user_id": payload.user_id, "emirates_id": payload.emirates_id_number })
                .to_string(),
        )
        .await;
    Json(json!({ "success": true, "user": updated })).into_response()
}

#[derive(Deserialize)]
pub struct GoogleAuthReq {
    pub id_token: String,
}

#[derive(Deserialize)]
struct GoogleTokenInfo {
    aud: String,
    iss: String,
    sub: String,
    email: String,
    name: Option<String>,
    email_verified: serde_json::Value,
}

pub async fn auth_google(
    State(state): State<AppState>,
    Json(payload): Json<GoogleAuthReq>,
) -> impl IntoResponse {
    let client_id = match std::env::var("GOOGLE_CLIENT_ID") {
        Ok(client_id) if !client_id.trim().is_empty() => client_id,
        _ => {
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "success": false, "error": "Google sign-in is not configured on this server" })),
            )
                .into_response();
        }
    };

    let token_info = match reqwest::Client::new()
        .get("https://oauth2.googleapis.com/tokeninfo")
        .query(&[("id_token", payload.id_token.as_str())])
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => match response.json::<GoogleTokenInfo>().await {
            Ok(token_info) => token_info,
            Err(_) => {
                return (
                    axum::http::StatusCode::UNAUTHORIZED,
                    Json(json!({ "success": false, "error": "Google returned an invalid sign-in token" })),
                )
                    .into_response();
            }
        },
        Ok(_) => {
            return (
                axum::http::StatusCode::UNAUTHORIZED,
                Json(json!({ "success": false, "error": "Google sign-in token is invalid or expired" })),
            )
                .into_response();
        }
        Err(_) => {
            return (
                axum::http::StatusCode::BAD_GATEWAY,
                Json(json!({ "success": false, "error": "Could not verify sign-in with Google" })),
            )
                .into_response();
        }
    };

    let email_is_verified = token_info.email_verified == serde_json::Value::Bool(true)
        || token_info.email_verified == serde_json::Value::String("true".to_string());
    let issuer_is_valid = token_info.iss == "accounts.google.com"
        || token_info.iss == "https://accounts.google.com";
    if token_info.aud != client_id
        || !issuer_is_valid
        || !email_is_verified
        || token_info.sub.trim().is_empty()
        || token_info.email.trim().is_empty()
    {
        return (
            axum::http::StatusCode::UNAUTHORIZED,
            Json(json!({ "success": false, "error": "Google sign-in token could not be verified" })),
        )
            .into_response();
    }

    let user_id = format!("user-{}", token_info.email.replace('@', "-").replace('.', "-"));
    let display_name = token_info.name.unwrap_or_else(|| token_info.email.clone());

    // Check if user already exists in DB to preserve their manually configured role
    let existing_role = db_try!(state
        .db
        .get_user(&user_id))
        .or(db_try!(state.db.get_user_by_email(&token_info.email)))
        .map(|u| u.role)
        .unwrap_or_else(|| "user".to_string());

    let user_profile = db_try!(state.db.upsert_user(
        &user_id,
        &display_name,
        &token_info.email,
        &existing_role,
        "google",
    ));

    let token = match create_jwt_token(&user_profile.id, &user_profile.email, &user_profile.role) {
        Ok(t) => t,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "success": false, "error": e })),
            )
                .into_response();
        }
    };

    (
        axum::http::StatusCode::OK,
        Json(json!({ "success": true, "token": token, "user": user_profile })),
    )
        .into_response()
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub struct EmailLoginReq {
    pub email: String,
    pub password: String,
    pub name: Option<String>,
}

pub async fn auth_email(
    State(state): State<AppState>,
    Json(payload): Json<EmailLoginReq>,
) -> impl IntoResponse {
    let user_id = format!("user-{}", payload.email.replace('@', "-").replace('.', "-"));
    let display_name = payload.name.unwrap_or_default();

    // Check if user already exists in DB to preserve their manually configured role
    let existing_role = db_try!(state
        .db
        .get_user(&user_id))
        .or(db_try!(state.db.get_user_by_email(&payload.email)))
        .map(|u| u.role)
        .unwrap_or_else(|| "user".to_string());

    let user_profile = db_try!(state.db.upsert_user(
        &user_id,
        &display_name,
        &payload.email,
        &existing_role,
        "email",
    ));

    let token = match create_jwt_token(&user_profile.id, &user_profile.email, &user_profile.role) {
        Ok(t) => t,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "success": false, "error": e })),
            )
                .into_response();
        }
    };

    (
        axum::http::StatusCode::OK,
        Json(json!({ "success": true, "token": token, "user": user_profile })),
    )
        .into_response()
}

pub async fn auth_me(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> impl IntoResponse {
    let headers = req.headers();
    let user = if let Some(auth_header) = headers.get("Authorization") {
        if let Ok(auth_str) = auth_header.to_str() {
            if auth_str.starts_with("Bearer ") {
                let token = &auth_str[7..];
                if let Ok(claims) = verify_jwt_token(token) {
                    db_try!(state.db.get_user(&claims.sub))
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };
    Json(json!({ "success": true, "user": user })).into_response()
}

pub async fn auth_logout() -> impl IntoResponse {
    Json(json!({ "success": true, "message": "Logged out successfully" })).into_response()
}
