use crate::models::{AudioSpeaker, S3DumpData};
use config::AppConfig;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

const MAX_PACKET_GAP_SAMPLES: usize = 16_000 / 50;

fn r2_origin_url(cfg: &AppConfig) -> String {
    if cfg.r2_endpoint_url.contains(".r2.cloudflarestorage.com") {
        cfg.r2_endpoint_url.trim_end_matches('/').to_string()
    } else {
        format!("https://{}.r2.cloudflarestorage.com", cfg.r2_account_id)
    }
}

pub fn build_conversation_wav(chunks: &mut [S3DumpData]) -> Result<Vec<u8>, String> {
    let mut user_track = Vec::new();
    let mut assistant_track = Vec::new();

    chunks.sort_by_key(|chunk| match chunk {
        S3DumpData::Audio {
            elapsed_samples, ..
        } => *elapsed_samples,
        _ => 0,
    });

    for chunk in chunks {
        let S3DumpData::Audio {
            samples,
            sample_rate,
            channels,
            elapsed_samples,
            speaker,
            ..
        } = chunk
        else {
            continue;
        };

        if *sample_rate == 0 || *channels == 0 {
            continue;
        }

        let mono = downmix_and_resample(samples, *sample_rate, *channels);
        if mono.is_empty() {
            continue;
        }
        let target = match speaker {
            AudioSpeaker::User => &mut user_track,
            AudioSpeaker::Assistant => &mut assistant_track,
        };
        let timestamp_sample = usize::try_from(*elapsed_samples).unwrap_or(usize::MAX);
        // Snap small scheduling gaps to the previous frame boundary to avoid clicks
        // and tiny silent gaps. Keep longer gaps as real silence between turns.
        let start = if timestamp_sample.saturating_sub(target.len()) <= MAX_PACKET_GAP_SAMPLES {
            target.len()
        } else {
            timestamp_sample.max(target.len())
        };
        let end = start.saturating_add(mono.len());
        if target.len() < end {
            target.resize(end, 0);
        }
        target[start..end].copy_from_slice(&mono);
    }

    let sample_count = user_track.len().max(assistant_track.len());
    if sample_count == 0 {
        return Err("No audio frames were captured for this call".to_string());
    }

    let data_size = sample_count
        .checked_mul(4)
        .and_then(|size| u32::try_from(size).ok())
        .filter(|size| *size <= u32::MAX - 36)
        .ok_or_else(|| "Call recording exceeds the WAV file size limit".to_string())?;
    let mut wav = Vec::with_capacity(data_size as usize + 44);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36u32 + data_size).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16_000u32.to_le_bytes());
    wav.extend_from_slice(&64_000u32.to_le_bytes());
    wav.extend_from_slice(&4u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    for index in 0..sample_count {
        wav.extend_from_slice(&user_track.get(index).copied().unwrap_or(0).to_le_bytes());
        wav.extend_from_slice(
            &assistant_track
                .get(index)
                .copied()
                .unwrap_or(0)
                .to_le_bytes(),
        );
    }
    Ok(wav)
}

fn downmix_and_resample(samples: &[i16], sample_rate: u32, channels: u16) -> Vec<i16> {
    let channel_count = channels as usize;
    let mono: Vec<i16> = samples
        .chunks_exact(channel_count)
        .map(|frame| {
            let sum: i64 = frame.iter().map(|sample| i64::from(*sample)).sum();
            (sum / channel_count as i64) as i16
        })
        .collect();
    if sample_rate == 16_000 {
        return mono;
    }
    let output_len = mono.len().saturating_mul(16_000) / sample_rate as usize;
    resample_mono(&mono, sample_rate, 16_000, output_len)
}

fn resample_mono(samples: &[i16], source_rate: u32, target_rate: u32, output_len: usize) -> Vec<i16> {
    const SINC_RADIUS: isize = 16;
    let cutoff = (target_rate as f64 / source_rate as f64).min(1.0) * 0.95;

    (0..output_len)
        .map(|output_index| {
            let source_position = output_index as f64 * source_rate as f64 / target_rate as f64;
            let center_index = source_position.floor() as isize;
            let first_index = (center_index - SINC_RADIUS).max(0) as usize;
            let last_index = (center_index + SINC_RADIUS + 1).max(0) as usize;
            let last_index = last_index.min(samples.len());
            let mut weighted_sample = 0.0;
            let mut weight_sum = 0.0;

            for (sample_index, sample) in samples
                .iter()
                .enumerate()
                .take(last_index)
                .skip(first_index)
            {
                let distance = sample_index as f64 - source_position;
                if distance.abs() >= SINC_RADIUS as f64 {
                    continue;
                }
                let window_position = distance / SINC_RADIUS as f64;
                let window = 0.5 * (1.0 + (std::f64::consts::PI * window_position).cos());
                let scaled_distance = cutoff * distance;
                let sinc = if scaled_distance.abs() < f64::EPSILON {
                    1.0
                } else {
                    (std::f64::consts::PI * scaled_distance).sin()
                        / (std::f64::consts::PI * scaled_distance)
                };
                let weight = cutoff * sinc * window;
                weighted_sample += f64::from(*sample) * weight;
                weight_sum += weight;
            }

            if weight_sum.abs() < f64::EPSILON {
                return 0;
            }
            (weighted_sample / weight_sum)
                .round()
                .clamp(i16::MIN as f64, i16::MAX as f64) as i16
        })
        .collect()
}

pub async fn upload_conversation(cfg: &AppConfig, session_id: &str, wav: &[u8]) -> Result<String, String> {
    upload_object(cfg, session_id, wav, "conversation.wav", "audio/wav").await
}

pub async fn upload_transcript(cfg: &AppConfig, session_id: &str, transcript: &str) -> Result<String, String> {
    upload_object(cfg, session_id, transcript.as_bytes(), "transcript.txt", "text/plain; charset=utf-8").await
}

async fn upload_object(
    cfg: &AppConfig,
    session_id: &str,
    payload: &[u8],
    filename: &str,
    content_type: &str,
) -> Result<String, String> {
    let call_key: String = session_id
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
        .collect();
    if call_key.is_empty() {
        return Err("Call ID cannot be used as an R2 object key".to_string());
    }

    let origin = r2_origin_url(cfg);
    let object_key = format!("calls/{}/{}", call_key, filename);
    let encoded_key = percent_encode(object_key.as_bytes(), true);
    let encoded_bucket = percent_encode(cfg.r2_bucket_name.as_bytes(), false);
    let url = format!("{}/{}/{}", origin, encoded_bucket, encoded_key);

    let parsed_url =
        reqwest::Url::parse(&url).map_err(|error| format!("Invalid R2 endpoint URL: {}", error))?;
    let host = parsed_url
        .host_str()
        .ok_or_else(|| "R2 endpoint URL is missing a host".to_string())?;
    let host = match parsed_url.port() {
        Some(port) => format!("{}:{}", host, port),
        None => host.to_string(),
    };

    let now = chrono::Utc::now();
    let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date_stamp = now.format("%Y%m%d").to_string();
    let payload_hash = sha256_hex(payload);
    let canonical_uri = parsed_url.path();
    let signed_headers = "content-type;host;x-amz-content-sha256;x-amz-date";
    let canonical_headers = format!(
        "content-type:{}\nhost:{}\nx-amz-content-sha256:{}\nx-amz-date:{}\n",
        content_type, host, payload_hash, amz_date
    );
    let canonical_request = format!(
        "PUT\n{}\n\n{}\n{}\n{}",
        canonical_uri,
        canonical_headers,
        signed_headers,
        payload_hash
    );
    let scope = format!("{}/auto/s3/aws4_request", date_stamp);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{}\n{}",
        amz_date,
        scope,
        sha256_hex(canonical_request.as_bytes())
    );
    let signature = sign_request(&cfg.r2_secret_access_key, &date_stamp, &string_to_sign)?;
    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
        cfg.r2_access_key_id, scope, signed_headers, signature
    );

    let response = reqwest::Client::new()
        .put(parsed_url)
        .header(reqwest::header::CONTENT_TYPE, content_type)
        .header("host", host)
        .header("x-amz-content-sha256", payload_hash)
        .header("x-amz-date", amz_date.clone())
        .header("authorization", authorization)
        .body(payload.to_vec())
        .timeout(std::time::Duration::from_secs(45))
        .send()
        .await
        .map_err(|error| format!("R2 upload request failed: {}", error))?;
    if !response.status().is_success() {
        let status = response.status();
        let response_body = response.text().await.unwrap_or_default();
        let response_detail = response_body.chars().take(1000).collect::<String>();
        return Err(format!(
            "R2 rejected the recording upload ({}): {}",
            status,
            if response_detail.is_empty() {
                "R2 returned an empty error body".to_string()
            } else {
                response_detail
            }
        ));
    }

    presign_get_url(
        &origin,
        &encoded_bucket,
        &encoded_key,
        &cfg.r2_access_key_id,
        &cfg.r2_secret_access_key,
        &date_stamp,
        &amz_date,
    )
}

pub fn presign_conversation_url(cfg: &AppConfig, session_id: &str) -> Result<String, String> {
    let call_key: String = session_id
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
        .collect();
    if call_key.is_empty() {
        return Err("Call ID cannot be used as an R2 object key".to_string());
    }
    let origin = r2_origin_url(cfg);
    let encoded_bucket = percent_encode(cfg.r2_bucket_name.as_bytes(), false);
    let object_key = format!("calls/{}/conversation.wav", call_key);
    let encoded_key = percent_encode(object_key.as_bytes(), true);
    let now = chrono::Utc::now();
    presign_get_url(
        &origin,
        &encoded_bucket,
        &encoded_key,
        &cfg.r2_access_key_id,
        &cfg.r2_secret_access_key,
        &now.format("%Y%m%d").to_string(),
        &now.format("%Y%m%dT%H%M%SZ").to_string(),
    )
}

fn presign_get_url(
    origin: &str,
    bucket: &str,
    key: &str,
    access_key: &str,
    secret_key: &str,
    date_stamp: &str,
    amz_date: &str,
) -> Result<String, String> {
    let url = format!("{}/{}/{}", origin, bucket, key);
    let parsed_url =
        reqwest::Url::parse(&url).map_err(|error| format!("Invalid R2 endpoint URL: {}", error))?;
    let host = parsed_url
        .host_str()
        .ok_or_else(|| "R2 endpoint URL is missing a host".to_string())?;
    let host = match parsed_url.port() {
        Some(port) => format!("{}:{}", host, port),
        None => host.to_string(),
    };
    let scope = format!("{}/auto/s3/aws4_request", date_stamp);
    let mut query_parts = vec![
        ("X-Amz-Algorithm", "AWS4-HMAC-SHA256".to_string()),
        (
            "X-Amz-Credential",
            percent_encode(format!("{}/{}", access_key, scope).as_bytes(), false),
        ),
        ("X-Amz-Date", amz_date.to_string()),
        ("X-Amz-Expires", "604800".to_string()),
        ("X-Amz-SignedHeaders", "host".to_string()),
    ];
    query_parts.sort_by_key(|(key, _)| *key);
    let canonical_query = query_parts
        .iter()
        .map(|(key, value)| format!("{}={}", key, value))
        .collect::<Vec<_>>()
        .join("&");
    let canonical_request = format!(
        "GET\n{}\n{}\nhost:{}\n\nhost\nUNSIGNED-PAYLOAD",
        parsed_url.path(),
        canonical_query,
        host
    );
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{}\n{}",
        amz_date,
        scope,
        sha256_hex(canonical_request.as_bytes())
    );
    let signature = sign_request(secret_key, date_stamp, &string_to_sign)?;
    Ok(format!(
        "{}?{}&X-Amz-Signature={}",
        url, canonical_query, signature
    ))
}

fn sign_request(secret: &str, date_stamp: &str, string_to_sign: &str) -> Result<String, String> {
    let date_key = hmac_sha256(format!("AWS4{}", secret).as_bytes(), date_stamp.as_bytes())?;
    let region_key = hmac_sha256(&date_key, b"auto")?;
    let service_key = hmac_sha256(&region_key, b"s3")?;
    let signing_key = hmac_sha256(&service_key, b"aws4_request")?;
    Ok(to_hex(&hmac_sha256(
        &signing_key,
        string_to_sign.as_bytes(),
    )?))
}

fn hmac_sha256(key: &[u8], value: &[u8]) -> Result<Vec<u8>, String> {
    let mut mac = HmacSha256::new_from_slice(key)
        .map_err(|_| "Could not initialize R2 request signing".to_string())?;
    mac.update(value);
    Ok(mac.finalize().into_bytes().to_vec())
}

fn sha256_hex(value: &[u8]) -> String {
    to_hex(&Sha256::digest(value))
}

fn to_hex(value: &[u8]) -> String {
    value.iter().map(|byte| format!("{:02x}", byte)).collect()
}

fn percent_encode(value: &[u8], preserve_slash: bool) -> String {
    value
        .iter()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (*byte as char).to_string()
            }
            b'/' if preserve_slash => "/".to_string(),
            _ => format!("%{:02X}", byte),
        })
        .collect::<Vec<_>>()
        .join("")
}
