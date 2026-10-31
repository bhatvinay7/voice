use axum::{
    extract::{FromRequestParts, Request},
    http::{request::Parts, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::AppState;
use db::UserProfile;

static INIT_CRYPTO: std::sync::Once = std::sync::Once::new();

pub fn init_crypto_provider() {
    INIT_CRYPTO.call_once(|| {
        let _ = jsonwebtoken::crypto::CryptoProvider::install_default(
            &jsonwebtoken::crypto::rust_crypto::DEFAULT_PROVIDER,
        );
    });
}

pub fn get_jwt_secret() -> Result<Vec<u8>, String> {
    std::env::var("JWT_SECRET")
        .ok().filter(|s| !s.trim().is_empty()).map(String::into_bytes)
        .ok_or_else(|| "JWT_SECRET environment variable is missing from .env".to_string())
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JwtClaims {
    pub sub: String,       // user_id
    pub email: String,     // user email
    pub role: String,      // "admin" or "user"
    pub exp: usize,        // expiration epoch timestamp
    pub iat: usize,        // issued-at epoch timestamp
}

pub fn create_jwt_token(user_id: &str, email: &str, role: &str) -> Result<String, String> {
    init_crypto_provider();
    let secret = get_jwt_secret()?;
    let now = chrono::Utc::now().timestamp() as usize;
    let exp = now + 7 * 24 * 3600; // 7 days expiration
    let claims = JwtClaims {
        sub: user_id.to_string(),
        email: email.to_string(),
        role: role.to_string(),
        exp,
        iat: now,
    };
    jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(&secret),
    )
    .map_err(|e| format!("Failed to create JWT token: {}", e))
}

pub fn verify_jwt_token(token: &str) -> Result<JwtClaims, String> {
    init_crypto_provider();
    let secret = get_jwt_secret()?;
    // Decode as cryptographic HMAC-SHA256 JWT using secret from .env
    let validation = jsonwebtoken::Validation::default();
    jsonwebtoken::decode::<JwtClaims>(
        token,
        &jsonwebtoken::DecodingKey::from_secret(&secret),
        &validation,
    )
    .map(|token_data| token_data.claims)
    .map_err(|e| format!("Invalid or expired JWT token: {}", e))
}

/// Axum Middleware checking that the request has a valid JWT with role == "admin"
pub async fn require_admin_role_middleware(
    mut req: Request,
    next: Next,
) -> Response {
    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok());

    let auth_str = match auth_header {
        Some(s) if s.starts_with("Bearer ") => &s[7..],
        _ => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "success": false,
                    "error": "Unauthorized: Missing Authorization header"
                })),
            )
                .into_response();
        }
    };

    let claims = match verify_jwt_token(auth_str) {
        Ok(c) => c,
        Err(_) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "success": false,
                    "error": "Unauthorized: Invalid or expired JWT token"
                })),
            )
                .into_response();
        }
    };

    // Check the role strictly from the JWT!
    if claims.role != "admin" {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({
                "success": false,
                "error": "Forbidden: Administrator role required"
            })),
        )
            .into_response();
    }

    req.extensions_mut().insert(claims);

    next.run(req).await
}

/// Axum Middleware checking that the request has a valid JWT (any authenticated role)
#[allow(dead_code)]
pub async fn require_auth_middleware(
    mut req: Request,
    next: Next,
) -> Response {
    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok());

    let auth_str = match auth_header {
        Some(s) if s.starts_with("Bearer ") => &s[7..],
        _ => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "success": false,
                    "error": "Unauthorized: Missing Authorization header"
                })),
            )
                .into_response();
        }
    };

    let claims = match verify_jwt_token(auth_str) {
        Ok(c) => c,
        Err(_) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "success": false,
                    "error": "Unauthorized: Invalid or expired JWT token"
                })),
            )
                .into_response();
        }
    };

    req.extensions_mut().insert(claims);

    next.run(req).await
}

/// Extractor for any authenticated user
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AuthUser(pub UserProfile);

/// Extractor for administrator user (strictly role == "admin")
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AdminUser(pub UserProfile);

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let claims = if let Some(c) = parts.extensions.get::<JwtClaims>() {
            c.clone()
        } else {
            let auth_header = parts
                .headers
                .get("Authorization")
                .and_then(|h| h.to_str().ok());

            let auth_str = match auth_header {
                Some(s) if s.starts_with("Bearer ") => &s[7..],
                _ => {
                    return Err((
                        StatusCode::UNAUTHORIZED,
                        Json(json!({
                            "success": false,
                            "error": "Unauthorized: Authentication required"
                        })),
                    )
                        .into_response());
                }
            };

            match verify_jwt_token(auth_str) {
                Ok(c) => c,
                Err(_) => {
                    return Err((
                        StatusCode::UNAUTHORIZED,
                        Json(json!({
                            "success": false,
                            "error": "Unauthorized: Invalid or expired JWT token"
                        })),
                    )
                        .into_response());
                }
            }
        };

        let user = match state.db.get_user(&claims.sub).await.map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"success": false, "error": "Database unavailable"}))).into_response())? {
            Some(u) => u,
            None => state.db.upsert_user(
                &claims.sub,
                "User",
                &claims.email,
                &claims.role,
                "jwt",
            ).await.map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"success": false, "error": "Database unavailable"}))).into_response())?,
        };

        Ok(AuthUser(user))
    }
}

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let claims = if let Some(c) = parts.extensions.get::<JwtClaims>() {
            c.clone()
        } else {
            let auth_header = parts
                .headers
                .get("Authorization")
                .and_then(|h| h.to_str().ok());

            let auth_str = match auth_header {
                Some(s) if s.starts_with("Bearer ") => &s[7..],
                _ => {
                    return Err((
                        StatusCode::UNAUTHORIZED,
                        Json(json!({
                            "success": false,
                            "error": "Unauthorized: Authentication required"
                        })),
                    )
                        .into_response());
                }
            };

            match verify_jwt_token(auth_str) {
                Ok(c) => c,
                Err(_) => {
                    return Err((
                        StatusCode::UNAUTHORIZED,
                        Json(json!({
                            "success": false,
                            "error": "Unauthorized: Invalid or expired JWT token"
                        })),
                    )
                        .into_response());
                }
            }
        };

        let user = match state.db.get_user(&claims.sub).await.map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"success": false, "error": "Database unavailable"}))).into_response())? {
            Some(u) => u,
            None => return Err((StatusCode::UNAUTHORIZED, Json(json!({"success": false, "error": "Unknown user"}))).into_response()),
        };

        // Check Neon on every protected request. A role change takes effect
        // immediately instead of waiting for an old JWT to expire.
        if user.role != "admin" {
            return Err((
                StatusCode::FORBIDDEN,
                Json(json!({
                    "success": false,
                    "error": "Forbidden: Administrator role required"
                })),
            ).into_response());
        }

        Ok(AdminUser(user))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_env() {
        let _ = dotenvy::dotenv();
        let _ = dotenvy::from_path(".env");
        let _ = dotenvy::from_path("../.env");
        let _ = dotenvy::from_path("../../.env");
        if std::env::var("JWT_SECRET").is_err() {
            std::env::set_var("JWT_SECRET", "uae-real-estate-rbac-jwt-secret-key-2026-prod-grade");
        }
    }

    #[test]
    fn test_jwt_token_creation_and_claims_verification() {
        setup_test_env();
        let token = create_jwt_token("user-admin-1", "admin@realestate.ae", "admin")
            .expect("Token creation should succeed");
        
        let claims = verify_jwt_token(&token).expect("Token verification should succeed");
        assert_eq!(claims.sub, "user-admin-1");
        assert_eq!(claims.email, "admin@realestate.ae");
        assert_eq!(claims.role, "admin");
        assert!(claims.exp > claims.iat);
    }

    #[test]
    fn test_jwt_user_vs_admin_role_check() {
        setup_test_env();
        let admin_token = create_jwt_token("user-adm", "adm@uae.ae", "admin").unwrap();
        let user_token = create_jwt_token("user-std", "user@uae.ae", "user").unwrap();

        let admin_claims = verify_jwt_token(&admin_token).unwrap();
        assert_eq!(admin_claims.role, "admin");

        let user_claims = verify_jwt_token(&user_token).unwrap();
        assert_eq!(user_claims.role, "user");
        assert_ne!(user_claims.role, "admin");
    }

    #[test]
    fn test_jwt_rejects_invalid_tokens() {
        setup_test_env();
        let bad_token = "invalid.token.payload";
        let res = verify_jwt_token(bad_token);
        assert!(res.is_err(), "Invalid JWT token must be rejected");
    }

    #[test]
    fn test_legacy_unverified_token_is_strictly_rejected() {
        setup_test_env();
        let legacy_token = "user-legacy-1:admin";
        let res = verify_jwt_token(legacy_token);
        assert!(res.is_err(), "Legacy unverified format must strictly be rejected");
    }
}

