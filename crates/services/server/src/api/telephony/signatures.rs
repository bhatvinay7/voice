use super::{constant_time_eq, TelephonyProvider};
use axum::http::{HeaderMap, Uri};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

pub(super) fn verify(
    config: &config::TelephonyConfig,
    provider: TelephonyProvider,
    uri: &Uri,
    headers: &HeaderMap,
    body: &[u8],
) -> bool {
    match provider {
        TelephonyProvider::Twilio => super::verify_twilio_signature(config, uri, headers, body),
        TelephonyProvider::Telnyx => verify_telnyx(config, headers, body),
        TelephonyProvider::Plivo => verify_plivo(config, uri, headers, body),
        TelephonyProvider::Vonage => verify_vonage(config, headers, body),
        TelephonyProvider::Sip | TelephonyProvider::Custom => {
            super::shared_secret_authorized(headers, config.webhook_secret.as_deref())
        }
        TelephonyProvider::Disabled => false,
    }
}

fn header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name)?.to_str().ok()
}

fn verify_telnyx(config: &config::TelephonyConfig, headers: &HeaderMap, body: &[u8]) -> bool {
    let Some(timestamp) = header(headers, "telnyx-timestamp") else {
        return false;
    };
    let Ok(time) = timestamp.parse::<i64>() else {
        return false;
    };
    let now = chrono::Utc::now().timestamp();
    if time < now.saturating_sub(300) || time > now.saturating_add(30) {
        return false;
    }
    let Some(signature) =
        header(headers, "telnyx-signature-ed25519").and_then(|value| STANDARD.decode(value).ok())
    else {
        return false;
    };
    let Some(key) = config
        .telnyx_public_key
        .as_deref()
        .and_then(|value| STANDARD.decode(value).ok())
    else {
        return false;
    };
    let mut signed = format!("{timestamp}|").into_bytes();
    signed.extend_from_slice(body);
    ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, key)
        .verify(&signed, &signature)
        .is_ok()
}

fn verify_plivo(
    config: &config::TelephonyConfig,
    uri: &Uri,
    headers: &HeaderMap,
    body: &[u8],
) -> bool {
    let Some(nonce) = header(headers, "x-plivo-signature-v3-nonce") else {
        return false;
    };
    let Some(signatures) = header(headers, "x-plivo-signature-v3") else {
        return false;
    };
    let (Some(base), Some(token)) = (&config.public_base_url, &config.plivo_auth_token) else {
        return false;
    };
    if nonce.is_empty() || nonce.len() > 256 {
        return false;
    }
    let mut fields = url::form_urlencoded::parse(body)
        .into_owned()
        .collect::<Vec<_>>();
    fields.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    let mut signed = format!("{}{uri}", base.trim_end_matches('/'));
    for (key, value) in fields {
        signed.push_str(&key);
        signed.push_str(&value);
    }
    signed.push_str(nonce);
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(token.as_bytes()) else {
        return false;
    };
    mac.update(signed.as_bytes());
    let expected = STANDARD.encode(mac.finalize().into_bytes());
    signatures
        .split(',')
        .any(|value| constant_time_eq(value.trim().as_bytes(), expected.as_bytes()))
}

#[derive(serde::Deserialize)]
struct VonageClaims {
    api_key: String,
    iat: i64,
    payload_hash: String,
}

fn verify_vonage(config: &config::TelephonyConfig, headers: &HeaderMap, body: &[u8]) -> bool {
    let Some(token) =
        header(headers, "authorization").and_then(|value| value.strip_prefix("Bearer "))
    else {
        return false;
    };
    let (Some(secret), Some(api_key)) = (&config.vonage_signature_secret, &config.vonage_api_key)
    else {
        return false;
    };
    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256);
    validation.set_required_spec_claims(&["iat", "iss"]);
    validation.set_issuer(&["Vonage"]);
    let Ok(decoded) = jsonwebtoken::decode::<VonageClaims>(
        token,
        &jsonwebtoken::DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    ) else {
        return false;
    };
    let claims = decoded.claims;
    let now = chrono::Utc::now().timestamp();
    let hash = format!("{:x}", Sha256::digest(body));
    claims.iat >= now.saturating_sub(300)
        && claims.iat <= now.saturating_add(30)
        && constant_time_eq(claims.api_key.as_bytes(), api_key.as_bytes())
        && constant_time_eq(claims.payload_hash.as_bytes(), hash.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::super::tests::state;
    use super::*;
    use ring::signature::KeyPair;

    #[test]
    fn native_providers_reject_shared_secret_as_signature_substitute() {
        for provider in [
            TelephonyProvider::Twilio,
            TelephonyProvider::Telnyx,
            TelephonyProvider::Vonage,
            TelephonyProvider::Plivo,
        ] {
            let (state, _) = state(provider);
            let mut headers = HeaderMap::new();
            headers.insert("x-telephony-secret", "media-test-secret".parse().unwrap());
            assert!(!verify(
                &state.config.telephony,
                provider,
                &Uri::from_static("/incoming"),
                &headers,
                b"{}"
            ));
        }
    }

    #[test]
    fn twilio_and_plivo_signatures_bind_url_and_decoded_body() {
        let body = b"CallSid=CA123&From=%2B1555&CallerName=Zo%C3%AB";
        for provider in [TelephonyProvider::Twilio, TelephonyProvider::Plivo] {
            let (state, _) = state(provider);
            let uri = Uri::from_static("/incoming");
            let signed = "https://voice.example.com/incomingCallSidCA123CallerNameZoëFrom+1555";
            let mut headers = HeaderMap::new();
            if provider == TelephonyProvider::Twilio {
                let mut mac = Hmac::<sha1::Sha1>::new_from_slice(b"twilio-test-secret").unwrap();
                mac.update(signed.as_bytes());
                headers.insert(
                    "x-twilio-signature",
                    STANDARD
                        .encode(mac.finalize().into_bytes())
                        .parse()
                        .unwrap(),
                );
            } else {
                let mut mac = Hmac::<Sha256>::new_from_slice(b"plivo-test-secret").unwrap();
                mac.update(format!("{signed}test-nonce").as_bytes());
                headers.insert("x-plivo-signature-v3-nonce", "test-nonce".parse().unwrap());
                headers.insert(
                    "x-plivo-signature-v3",
                    STANDARD
                        .encode(mac.finalize().into_bytes())
                        .parse()
                        .unwrap(),
                );
            }
            assert!(verify(
                &state.config.telephony,
                provider,
                &uri,
                &headers,
                body
            ));
            assert!(!verify(
                &state.config.telephony,
                provider,
                &uri,
                &headers,
                b"CallSid=other"
            ));
            assert!(!verify(
                &state.config.telephony,
                provider,
                &Uri::from_static("/other"),
                &headers,
                body
            ));
        }
    }

    #[test]
    fn telnyx_signature_checks_body_and_timestamp() {
        let (mut state, _) = state(TelephonyProvider::Telnyx);
        let key = ring::signature::Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
        state.config.telephony.telnyx_public_key = Some(STANDARD.encode(key.public_key().as_ref()));
        let timestamp = chrono::Utc::now().timestamp().to_string();
        let body = br#"{"data":{"event_type":"call.initiated"}}"#;
        let mut signed = format!("{timestamp}|").into_bytes();
        signed.extend_from_slice(body);
        let mut headers = HeaderMap::new();
        headers.insert("telnyx-timestamp", timestamp.parse().unwrap());
        headers.insert(
            "telnyx-signature-ed25519",
            STANDARD.encode(key.sign(&signed).as_ref()).parse().unwrap(),
        );
        assert!(verify_telnyx(&state.config.telephony, &headers, body));
        assert!(!verify_telnyx(&state.config.telephony, &headers, b"{}"));
        headers.insert("telnyx-timestamp", "0".parse().unwrap());
        assert!(!verify_telnyx(&state.config.telephony, &headers, body));
    }

    #[test]
    fn vonage_jwt_checks_api_key_and_payload_hash() {
        crate::api::rbac::init_crypto_provider();
        let (state, _) = state(TelephonyProvider::Vonage);
        let now = chrono::Utc::now().timestamp();
        let body = br#"{"uuid":"call-a"}"#;
        let claims = serde_json::json!({"api_key":"vonage-test-key","iat":now,"exp":now+120,"iss":"Vonage","payload_hash":format!("{:x}",Sha256::digest(body))});
        let jwt = jsonwebtoken::encode(
            &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(b"vonage-test-secret"),
        )
        .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("authorization", format!("Bearer {jwt}").parse().unwrap());
        assert!(verify_vonage(&state.config.telephony, &headers, body));
        assert!(!verify_vonage(&state.config.telephony, &headers, b"{}"));
    }
}
