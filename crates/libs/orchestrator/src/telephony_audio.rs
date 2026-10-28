/// Twilio Media Streams currently use mono G.711 μ-law audio at 8 kHz.
/// Decode at the provider boundary; STT/VAD consumers receive linear PCM.
pub fn decode_mulaw_8khz(encoded: &[u8]) -> Vec<i16> {
    encoded.iter().copied().map(decode_mulaw_sample).collect()
}

/// Encode linear PCM as mono 8 kHz G.711 μ-law for Twilio's outbound media
/// messages. Callers are responsible for resampling to 8 kHz first.
pub fn encode_mulaw_8khz(pcm: &[i16]) -> Vec<u8> {
    pcm.iter().copied().map(encode_mulaw_sample).collect()
}

/// Convert linear PCM between telephony sample rates with linear
/// interpolation. It preserves duration and accepts mono samples only.
pub fn resample_mono_pcm(pcm: &[i16], from_hz: u32, to_hz: u32) -> Vec<i16> {
    if pcm.is_empty() || from_hz == 0 || to_hz == 0 {
        return Vec::new();
    }
    if from_hz == to_hz {
        return pcm.to_vec();
    }

    let output_len =
        ((pcm.len() as u128 * to_hz as u128) / from_hz as u128).min(usize::MAX as u128) as usize;
    (0..output_len)
        .map(|index| {
            let source_position = index as f64 * from_hz as f64 / to_hz as f64;
            let left = (source_position.floor() as usize).min(pcm.len() - 1);
            let right = (left + 1).min(pcm.len() - 1);
            let fraction = source_position - left as f64;
            let sample = pcm[left] as f64 * (1.0 - fraction) + pcm[right] as f64 * fraction;
            sample.round().clamp(i16::MIN as f64, i16::MAX as f64) as i16
        })
        .collect()
}

fn decode_mulaw_sample(byte: u8) -> i16 {
    let value = !byte;
    let sign = value & 0x80;
    let exponent = (value >> 4) & 0x07;
    let mantissa = value & 0x0f;
    let magnitude = (((mantissa as i32) << 3) + 0x84) << exponent;
    let sample = magnitude - 0x84;
    if sign != 0 {
        (-sample).clamp(i16::MIN as i32, i16::MAX as i32) as i16
    } else {
        sample.clamp(i16::MIN as i32, i16::MAX as i32) as i16
    }
}

fn encode_mulaw_sample(sample: i16) -> u8 {
    const BIAS: i32 = 0x84;
    const CLIP: i32 = 32635;

    let sample = sample as i32;
    let sign = if sample < 0 { 0x80 } else { 0 };
    let magnitude = sample.abs().min(CLIP) + BIAS;
    let exponent = (7..=14)
        .rev()
        .find(|shift| (magnitude & (1 << shift)) != 0)
        .unwrap_or(0);
    let mantissa = (magnitude >> (exponent - 4)) & 0x0f;
    !(sign | ((exponent - 7) << 4) | mantissa) as u8
}
