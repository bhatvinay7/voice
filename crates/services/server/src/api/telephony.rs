//! Phone-provider ingress kept separate from browser WebRTC call routes.
//!
//! Provider callbacks can only target the provider selected through
//! `TELEPHONY_PROVIDER`. A phone call is admitted directly to the
//! orchestrator queue; this module never creates a LiveKit token or sends a
//! provider payload through the web pub/sub channel.

use crate::AppState;
use axum::{
    body::Bytes,
    extract::{ws::WebSocketUpgrade, OriginalUri, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine as _,
};
use config::TelephonyProvider;
use hmac::{Hmac, Mac};
use orchestrator::QueueEvent;
use serde::Serialize;
use serde_json::{json, Value};
use sha1::Sha1;
use sha2::Sha256;
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc::error::TrySendError;

mod signatures;
mod telephony_media;

const MAX_PROVIDER_PAYLOAD_BYTES: usize = 64 * 1024;
const MAX_PROVIDER_ID_BYTES: usize = 256;
const MEDIA_CAPABILITY_SECONDS: u64 = 10 * 60;
type HmacSha256 = Hmac<Sha256>;
type HmacSha1 = Hmac<Sha1>;

#[derive(serde::Deserialize)]
pub(crate) struct MediaPath {
    pub provider: String,
    pub call_id: String,
    pub token: Option<String>,
}

#[derive(serde::Deserialize)]
pub(crate) struct MediaQuery {
    pub token: Option<String>,
}

#[derive(Serialize)]
struct TelephonyStatus {
    provider: &'static str,
    enabled: bool,
    credentials_configured: bool,
    media_ingress_configured: bool,
    phone_ingress_only: bool,
}

/// Exposes configuration state without returning credentials.
pub async fn status_handler(State(state): State<AppState>) -> impl IntoResponse {
    let config = &state.config.telephony;
    Json(TelephonyStatus {
        provider: config.provider.as_str(),
        enabled: config.provider != TelephonyProvider::Disabled,
        credentials_configured: config.provider_credentials_configured(),
        media_ingress_configured: config.media_ingress_configured(),
        phone_ingress_only: true,
    })
}

/// Receives a webhook from one of the supported phone providers.
///
/// Payload parsing intentionally handles both common form webhook fields and
/// JSON callbacks. It normalizes an event into `QueueEvent` and never calls
/// the browser/WebRTC API or broadcast bus.
pub async fn inbound_handler(
    Path(provider): Path<String>,
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let provider = match TelephonyProvider::parse(&provider) {
        Ok(TelephonyProvider::Disabled) | Err(_) => {
            return api_error(StatusCode::NOT_FOUND, "Unsupported telephony provider")
        }
        Ok(provider) => provider,
    };
    if provider != state.config.telephony.provider {
        return api_error(
            StatusCode::NOT_FOUND,
            "This provider is not active on this server",
        );
    }
    if !state.config.telephony.media_ingress_configured() {
        return api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Provider credentials, public base URL, or webhook secret are not configured",
        );
    }
    if body.len() > MAX_PROVIDER_PAYLOAD_BYTES {
        return api_error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Provider payload is too large",
        );
    }

    if !provider_request_authorized(&state.config.telephony, provider, &uri, &headers, &body) {
        return api_error(
            StatusCode::UNAUTHORIZED,
            "Provider webhook authorization failed",
        );
    }

    let callback = match normalize_callback(provider, &headers, &body) {
        Ok(callback) => callback,
        Err(error) => return api_error(StatusCode::BAD_REQUEST, &error),
    };
    let session_id = format!("phone:{}:{}", provider.as_str(), callback.call_id);
    match callback.kind {
        CallbackKind::End => {
            state.phone_media.close(&session_id);
            return match state
                .orch_tx
                .send(QueueEvent::SessionCancelled { session_id })
                .await
            {
                Ok(()) => StatusCode::NO_CONTENT.into_response(),
                Err(_) => api_error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Phone runtime is unavailable",
                ),
            };
        }
        CallbackKind::Ignore => return StatusCode::NO_CONTENT.into_response(),
        CallbackKind::Start => {}
    }
    if !state.phone_media.reserve(&session_id) {
        return match provider_accept_response(&state.config.telephony, provider, &session_id).await
        {
            Ok(response) => response,
            Err(error) => api_error(StatusCode::SERVICE_UNAVAILABLE, &error),
        };
    }
    if let Err(error) = state.slots.acquire(&session_id, chrono::Utc::now().timestamp()) {
        state.phone_media.close(&session_id);
        return api_error(StatusCode::SERVICE_UNAVAILABLE, &error);
    }
    let event = QueueEvent::SessionStarted {
        session_id: session_id.clone(),
        user_id: callback
            .caller
            .unwrap_or_else(|| "phone-caller".to_string()),
        language: callback.language,
    };
    match state.orch_tx.try_send(event) {
        Ok(()) => {
            let waiting_state = state.clone();
            let waiting_id = session_id.clone();
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                if waiting_state.phone_media.is_waiting(&waiting_id) {
                    waiting_state.phone_media.close(&waiting_id);
                    let _ = waiting_state
                        .orch_tx
                        .send(QueueEvent::SessionCancelled {
                            session_id: waiting_id,
                        })
                        .await;
                }
            });
            match provider_accept_response(&state.config.telephony, provider, &session_id).await {
                Ok(response) => response,
                Err(error) => {
                    state.phone_media.close(&session_id);
                    let _ = state
                        .orch_tx
                        .try_send(QueueEvent::SessionCancelled { session_id });
                    api_error(StatusCode::SERVICE_UNAVAILABLE, &error)
                }
            }
        }
        Err(error) => {
            state.phone_media.close(&session_id);
            let _ = state.slots.release(&session_id);
            let message = match error {
                TrySendError::Full(_) => "Phone session queue is full; retry the callback",
                TrySendError::Closed(_) => "Phone session runtime is unavailable",
            };
            api_error(StatusCode::SERVICE_UNAVAILABLE, message)
        }
    }
}

/// Accepts provider media on a separate WebSocket and sends it only to the
/// phone session actor. It has no browser subscriber or LiveKit publication.
pub async fn media_handler(
    Path(path): Path<MediaPath>,
    Query(query): Query<MediaQuery>,
    State(state): State<AppState>,
    websocket: WebSocketUpgrade,
) -> Response {
    let provider = match TelephonyProvider::parse(&path.provider) {
        Ok(TelephonyProvider::Disabled) | Err(_) => {
            return api_error(StatusCode::NOT_FOUND, "Unsupported telephony provider")
        }
        Ok(provider) => provider,
    };
    if provider != state.config.telephony.provider {
        return api_error(
            StatusCode::NOT_FOUND,
            "This provider is not active on this server",
        );
    }
    let session_id = format!("phone:{}:{}", provider.as_str(), path.call_id);
    let token = path
        .token
        .as_deref()
        .or(query.token.as_deref())
        .unwrap_or("");
    if !verify_media_capability(&state.config.telephony, provider, &session_id, token) {
        return api_error(
            StatusCode::UNAUTHORIZED,
            "Phone media capability was rejected",
        );
    }
    let (outbound, cancelled) = match state.phone_media.attach(&session_id) {
        Ok(session) => session,
        Err(error) => return api_error(StatusCode::CONFLICT, &error),
    };
    let failed_state = state.clone();
    let failed_id = session_id.clone();
    websocket
        .max_message_size(MAX_PROVIDER_PAYLOAD_BYTES)
        .max_frame_size(MAX_PROVIDER_PAYLOAD_BYTES)
        .on_failed_upgrade(move |_| {
            failed_state.phone_media.close(&failed_id);
            let _ = failed_state.orch_tx.try_send(QueueEvent::SessionCancelled {
                session_id: failed_id,
            });
        })
        .on_upgrade(move |socket| {
            telephony_media::run(socket, state, provider, session_id, outbound, cancelled)
        })
        .into_response()
}

#[derive(Debug)]
struct NormalizedCallback {
    call_id: String,
    caller: Option<String>,
    language: String,
    kind: CallbackKind,
}

fn normalize_callback(
    provider: TelephonyProvider,
    headers: &HeaderMap,
    body: &[u8],
) -> Result<NormalizedCallback, String> {
    let values = decode_callback_values(headers, body)?;
    let call_id = provider_call_id(provider, &values)
        .ok_or_else(|| "Provider call identifier is missing".to_string())?;
    if call_id.is_empty()
        || call_id.len() > MAX_PROVIDER_ID_BYTES
        || !call_id.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(byte, b'-' | b'_' | b'@' | b'.' | b'+' | b':' | b'=')
        })
    {
        return Err("Provider call identifier is invalid".to_string());
    }
    let caller = first_value(&values, &["from", "From", "caller", "caller_number"]);
    let language = first_value(&values, &["language", "Language"])
        .filter(|value| value.len() <= 32)
        .unwrap_or_else(|| "en".to_string());
    let kind = callback_kind(provider, &values);
    Ok(NormalizedCallback {
        kind,
        call_id,
        caller,
        language,
    })
}

fn decode_callback_values(
    headers: &HeaderMap,
    body: &[u8],
) -> Result<BTreeMap<String, String>, String> {
    if headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.contains("application/json"))
    {
        let payload: Value =
            serde_json::from_slice(body).map_err(|_| "Provider JSON is invalid".to_string())?;
        let mut values = BTreeMap::new();
        flatten_json("", &payload, &mut values);
        return Ok(values);
    }
    Ok(url::form_urlencoded::parse(body).into_owned().collect())
}

fn flatten_json(prefix: &str, value: &Value, values: &mut BTreeMap<String, String>) {
    match value {
        Value::Object(object) => {
            for (key, nested) in object {
                let name = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                flatten_json(&name, nested, values);
            }
        }
        Value::String(value) => {
            values.insert(prefix.to_string(), value.clone());
        }
        Value::Number(value) => {
            values.insert(prefix.to_string(), value.to_string());
        }
        _ => {}
    }
}

fn provider_call_id(
    provider: TelephonyProvider,
    values: &BTreeMap<String, String>,
) -> Option<String> {
    let keys: &[&str] = match provider {
        TelephonyProvider::Twilio => &["CallSid", "call_sid", "call_id"],
        TelephonyProvider::Telnyx => {
            &["data.payload.call_control_id", "call_control_id", "call_id"]
        }
        TelephonyProvider::Vonage => &["uuid", "conversation_uuid", "call_id"],
        TelephonyProvider::Plivo => &["CallUUID", "call_uuid", "call_id"],
        TelephonyProvider::Sip | TelephonyProvider::Custom => &["call_id", "session_id", "id"],
        TelephonyProvider::Disabled => &[],
    };
    first_value(values, keys)
}

fn first_value(values: &BTreeMap<String, String>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| values.get(*key).cloned())
        .filter(|value| !value.is_empty())
}

fn provider_request_authorized(
    config: &config::TelephonyConfig,
    provider: TelephonyProvider,
    uri: &axum::http::Uri,
    headers: &HeaderMap,
    body: &[u8],
) -> bool {
    signatures::verify(config, provider, uri, headers, body)
}

fn shared_secret_authorized(headers: &HeaderMap, secret: Option<&str>) -> bool {
    let Some(secret) = secret else {
        return false;
    };
    let provided = headers
        .get("x-telephony-secret")
        .or_else(|| headers.get("authorization"))
        .and_then(|value| value.to_str().ok())
        .map(|value| value.strip_prefix("Bearer ").unwrap_or(value));
    provided.is_some_and(|value| constant_time_eq(value.as_bytes(), secret.as_bytes()))
}

fn verify_twilio_signature(
    config: &config::TelephonyConfig,
    uri: &axum::http::Uri,
    headers: &HeaderMap,
    body: &[u8],
) -> bool {
    let Some(signature) = headers
        .get("x-twilio-signature")
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    let Some(token) = config.twilio_auth_token.as_deref() else {
        return false;
    };
    let Some(base_url) = config.public_base_url.as_deref() else {
        return false;
    };
    let mut values = match decode_callback_values(headers, body) {
        Ok(values) => values.into_iter().collect::<Vec<_>>(),
        Err(_) => return false,
    };
    values.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    let mut signed = format!("{}{}", base_url.trim_end_matches('/'), uri);
    for (key, value) in values {
        signed.push_str(&key);
        signed.push_str(&value);
    }
    let Ok(mut mac) = HmacSha1::new_from_slice(token.as_bytes()) else {
        return false;
    };
    mac.update(signed.as_bytes());
    let expected = STANDARD.encode(mac.finalize().into_bytes());
    constant_time_eq(expected.as_bytes(), signature.as_bytes())
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    for index in 0..left.len().max(right.len()) {
        difference |= left.get(index).copied().unwrap_or_default() as usize
            ^ right.get(index).copied().unwrap_or_default() as usize;
    }
    difference == 0
}

async fn provider_accept_response(
    config: &config::TelephonyConfig,
    provider: TelephonyProvider,
    session_id: &str,
) -> Result<Response, String> {
    let media_url = media_url(config, provider, session_id)?;
    let xml_media_url = xml_escape(&media_url);
    match provider {
        TelephonyProvider::Twilio => Ok((
            [(axum::http::header::CONTENT_TYPE, "text/xml")],
            format!("<Response><Connect><Stream url=\"{xml_media_url}\" /></Connect></Response>"),
        )
            .into_response()),
        TelephonyProvider::Vonage => Ok(Json(json!([{
            "action": "connect",
            "endpoint": [{"type": "websocket", "uri": media_url, "content-type": "audio/l16;rate=16000"}]
        }])).into_response()),
        TelephonyProvider::Plivo => Ok((
            [(axum::http::header::CONTENT_TYPE, "text/xml")],
            format!("<Response><Stream bidirectional=\"true\" keepCallAlive=\"true\" contentType=\"audio/x-mulaw;rate=8000\" audioTrack=\"inbound\">{xml_media_url}</Stream></Response>"),
        ).into_response()),
        TelephonyProvider::Telnyx => {
            answer_telnyx_call(config, session_id, &media_url).await?;
            Ok(axum::http::StatusCode::NO_CONTENT.into_response())
        }
        _ => Ok(Json(json!({"accepted": true, "session_id": session_id, "media_url": media_url, "transport": "phone"})).into_response()),
    }
}

async fn answer_telnyx_call(
    config: &config::TelephonyConfig,
    session_id: &str,
    media_url: &str,
) -> Result<(), String> {
    let api_key = config
        .telnyx_api_key
        .as_deref()
        .ok_or_else(|| "TELNYX_API_KEY must be configured".to_string())?;
    let call_control_id = session_id
        .splitn(3, ':')
        .nth(2)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Telnyx call-control ID is missing".to_string())?;
    let call_control_id =
        url::form_urlencoded::byte_serialize(call_control_id.as_bytes()).collect::<String>();
    let endpoint = format!("https://api.telnyx.com/v2/calls/{call_control_id}/actions/answer");
    let response = reqwest::Client::new()
        .post(endpoint)
        .bearer_auth(api_key)
        .json(&json!({
            "stream_url": media_url,
            "stream_track": "inbound_track",
            "stream_codec": "PCMU",
            "stream_bidirectional_mode": "rtp",
            "stream_bidirectional_codec": "PCMU",
            "stream_bidirectional_sampling_rate": 8000,
            "command_id": session_id,
        }))
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|_| "Telnyx answer command could not be sent".to_string())?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!(
            "Telnyx answer command returned HTTP {}",
            response.status()
        ))
    }
}

fn media_url(
    config: &config::TelephonyConfig,
    provider: TelephonyProvider,
    session_id: &str,
) -> Result<String, String> {
    let public_base_url = config
        .public_base_url
        .as_deref()
        .ok_or_else(|| "TELEPHONY_PUBLIC_BASE_URL must be configured".to_string())?;
    let websocket_base = if let Some(base) = public_base_url.strip_prefix("https://") {
        format!("wss://{base}")
    } else if let Some(base) = public_base_url.strip_prefix("http://") {
        format!("ws://{base}")
    } else {
        return Err("TELEPHONY_PUBLIC_BASE_URL must start with http:// or https://".to_string());
    };
    let expires_at = unix_now().saturating_add(MEDIA_CAPABILITY_SECONDS);
    let token = mint_media_capability(config, provider, session_id, expires_at)?;
    Ok(format!(
        "{}/api/telephony/{}/media/{}/{}",
        websocket_base.trim_end_matches('/'),
        provider.as_str(),
        url::form_urlencoded::byte_serialize(
            session_id
                .splitn(3, ':')
                .nth(2)
                .ok_or("Invalid phone session ID")?
                .as_bytes()
        )
        .collect::<String>(),
        token
    ))
}

fn mint_media_capability(
    config: &config::TelephonyConfig,
    provider: TelephonyProvider,
    session_id: &str,
    expires_at: u64,
) -> Result<String, String> {
    let secret = config
        .webhook_secret
        .as_deref()
        .ok_or_else(|| "TELEPHONY_WEBHOOK_SECRET must be configured".to_string())?;
    let payload = capability_payload(provider, session_id, expires_at);
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|_| "Could not initialize phone media signer".to_string())?;
    mac.update(payload.as_bytes());
    Ok(format!(
        "{expires_at}.{}",
        URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
    ))
}

fn verify_media_capability(
    config: &config::TelephonyConfig,
    provider: TelephonyProvider,
    session_id: &str,
    token: &str,
) -> bool {
    let Some((expiry, signature)) = token.split_once('.') else {
        return false;
    };
    let Ok(expiry) = expiry.parse::<u64>() else {
        return false;
    };
    if expiry < unix_now() || expiry > unix_now().saturating_add(MEDIA_CAPABILITY_SECONDS) {
        return false;
    }
    let Ok(expected) = mint_media_capability(config, provider, session_id, expiry) else {
        return false;
    };
    let Some((_, expected_signature)) = expected.split_once('.') else {
        return false;
    };
    constant_time_eq(signature.as_bytes(), expected_signature.as_bytes())
}

fn capability_payload(provider: TelephonyProvider, session_id: &str, expires_at: u64) -> String {
    format!("{}|{}|{}", provider.as_str(), session_id, expires_at)
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn api_error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({"success": false, "error": message}))).into_response()
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[derive(Debug, PartialEq, Eq)]
enum CallbackKind {
    Start,
    End,
    Ignore,
}

fn callback_kind(provider: TelephonyProvider, values: &BTreeMap<String, String>) -> CallbackKind {
    let status = first_value(
        values,
        &[
            "CallStatus",
            "status",
            "call_status",
            "data.event_type",
            "event",
        ],
    );
    match status.as_deref() {
        Some(
            "completed" | "failed" | "busy" | "no-answer" | "canceled" | "cancelled" | "rejected"
            | "timeout" | "hangup" | "call.hangup" | "stop",
        ) => CallbackKind::End,
        Some("call.initiated") if provider == TelephonyProvider::Telnyx => {
            if values
                .get("data.payload.direction")
                .is_some_and(|direction| direction != "incoming")
            {
                CallbackKind::Ignore
            } else {
                CallbackKind::Start
            }
        }
        _ if provider == TelephonyProvider::Telnyx => CallbackKind::Ignore,
        Some(_) if provider == TelephonyProvider::Vonage => CallbackKind::Ignore,
        None | Some("ringing" | "in-progress" | "queued" | "start" | "call.started") => {
            CallbackKind::Start
        }
        _ => CallbackKind::Ignore,
    }
}

#[cfg(test)]
mod tests;
