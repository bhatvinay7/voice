use serde::Deserialize;
use std::env;
use std::str::FromStr;

fn parse_env<T>(key: &str) -> Result<T, String>
where
    T: FromStr,
    T::Err: std::fmt::Display,
{
    let value = env::var(key).map_err(|_| format!("{} not set in environment", key))?;
    value
        .parse::<T>()
        .map_err(|error| format!("Invalid {}: {}", key, error))
}

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub port: u16,
    pub orchestrator_port: u16,
    pub agent_port: u16,
    pub redis_url: String,
    pub postgres_url: String,
    pub livekit_url: String,
    pub livekit_api_key: String,
    pub livekit_api_secret: String,
    pub stt_provider: String,
    pub llm_provider: String,
    pub tts_provider: String,
    pub openai_api_key: String,
    pub groq_api_key: String,
    pub elevenlabs_api_key: String,
    pub groq_stt_url: String,
    pub stt_model: String,
    pub openai_tts_url: String,
    pub r2_bucket_name: String,
    pub r2_account_id: String,
    pub r2_access_key_id: String,
    pub r2_secret_access_key: String,
    pub r2_endpoint_url: String,
    pub tone_model_api_key: String,
    pub upstash_vector_rest_url: String,
    pub upstash_vector_rest_token: String,
    pub node_env: String,
    pub jwt_secret: String,

    // Neo4j / Graph DB
    pub neo4j_uri: Option<String>,
    pub neo4j_username: Option<String>,
    pub neo4j_password: Option<String>,
    pub neo4j_database: Option<String>,

    // General Domain, Currency & KYC Verification Rules (Rules 14 & 15)
    pub app_domain_name: String,
    pub app_currency: String,
    pub kyc_document_name: String,
    pub kyc_required_for_booking: bool,
    pub kyc_unverified_message: String,
    pub domain_fallback_message: String,

    // Dynamic Filter & Action Keywords
    pub filter_primary_type_a: String,
    pub filter_primary_type_b: String,
    pub filter_type_a_synonyms: Vec<String>,
    pub filter_type_b_synonyms: Vec<String>,
    pub filter_feature_1_synonyms: Vec<String>,
    pub filter_feature_2_synonyms: Vec<String>,
    pub action_booking_keywords: Vec<String>,
    /// Short follow-up utterances that should be resolved against active
    /// session context instead of becoming a new search query.
    pub contextual_follow_up_phrases: Vec<String>,
    /// Template used when a short follow-up has no explicit subject. Supports
    /// the `__PROPERTY_TITLE__` placeholder.
    pub contextual_follow_up_clarification: String,

    // Pluggable LLM Model & Endpoint
    pub llm_api_key: String,
    pub llm_endpoint_url: String,
    pub llm_model: String,
    pub agent_system_prompt: String,
    pub agent_context_prompt_template: String,
    pub agent_edge_prompt_template: String,
    /// Fast-model prompt used to decide whether a final caller turn is a
    /// valid question, can be answered from graph context, or needs a tool.
    pub agent_turn_planner_prompt_template: String,
    pub agent_turn_router_prompt_template: String,
    pub agent_confirmation_planner_prompt_template: String,
    pub agent_reference_planner_prompt_template: String,
    pub agent_dependency_planner_prompt_template: String,
    /// Response-model prompt used after a tool result has been attached to
    /// the originating graph branch.
    pub agent_tool_result_prompt_template: String,
    /// Compact event-specific instruction for asynchronous graph enrichment.
    pub agent_graph_enrichment_prompt_template: String,
    /// Brief speech queued while a tool request is running; empty disables it.
    pub tool_wait_tts_prompt: String,
    pub tool_retry_tts_prompt: String,
    /// Spoken when a newer graph revision supersedes an in-flight answer.
    pub sts_context_update_ack: String,
    pub stt_prompt: String,
    /// BCP-47 language code sent to the Whisper STT API and the OpenAI Realtime
    /// transcription model.  Set `STT_LANGUAGE` in .env.  Empty string = auto-detect.
    /// Examples: "en", "ar", "hi", "fr", "de".
    pub stt_language: String,
    /// Optional BCP-47 code injected into the agent system prompt so the LLM
    /// always responds in the configured language.  Set `LLM_RESPONSE_LANGUAGE`.
    /// Empty string = let the LLM mirror the user's language.
    pub llm_response_language: String,
    pub vad_speech_probability_threshold: f32,
    pub vad_barge_in_probability_threshold: f32,
    pub vad_min_speech_amplitude: u16,
    /// Upper peak amplitude (u16 scale) for a chunk to be considered in the
    /// normal human speaking range.  Chunks louder than this or at the clip
    /// level are labelled `LoudNoise` and never open a turn or trigger barge-in.
    pub vad_max_speech_amplitude: u16,
    pub vad_energy_bypass_amplitude: u16, // low-energy speech bypass threshold
    pub vad_barge_in_amplitude: u16,
    pub vad_barge_in_min_chunks: u32,
    pub vad_end_of_speech_timeout_ms: u64,
    /// Rolling analysis window for the noise gate in milliseconds (1000–1300).
    pub vad_analysis_window_ms: u64,
    /// Minimum number of speech chunks inside the analysis window before a
    /// user turn is opened.
    pub vad_min_speech_chunks_in_window: u32,
    /// Peak amplitude at or above which a sample is counted as clipped.
    pub vad_clip_level: u16,
    /// Maximum crest factor (peak/rms) before a chunk is labelled impulsive noise.
    pub vad_max_crest_factor: f32,
    /// Minimum coefficient of variation of chunk RMS; below this = sustained tone.
    pub vad_min_energy_variation: f32,
    /// Fraction of LoudNoise chunks in a window that marks it as LoudNoiseOnly
    /// (no user turn started).
    pub vad_max_loud_ratio: f32,
    pub sts_text_jitter_ms: u64,
    pub sts_text_max_wait_ms: u64,
    pub graph_context_request_timeout_ms: u64,
    /// Per-attempt remote tool deadline. Cancellation always preempts this.
    pub orchestrator_tool_timeout_ms: u64,
    pub orchestrator_tool_max_retries: u32,
    pub orchestrator_retry_base_ms: u64,
    pub orchestrator_task_lease_ms: u64,
    pub decision_llm_api_key: String,
    pub decision_llm_endpoint_url: String,
    pub decision_llm_model: String,
    /// Low-latency model used for query rewriting and orchestration routing.
    pub fast_model_api_key: String,
    pub fast_model_endpoint_url: String,
    pub fast_model: String,
    /// Comma-separated business vocabulary injected into RAG query rewriting.
    pub rag_business_keywords: Vec<String>,
    pub tts_model: String,
    pub tts_fallback_model: String,
    pub tts_voice: String,
    pub recording_max_buffer_bytes: usize,

    // Dynamic Server Text Messages (Zero Hardcoding Rule)
    pub unrelated_query_message: String,
    pub property_match_message_template: String,
    pub rag_search_success_message: String,
    pub contract_not_found_message: String,
    /// Phone-provider ingress is kept separate from browser LiveKit calls.
    pub telephony: TelephonyConfig,
    /// Message spoken when the AI cannot understand the user due to noise.
    pub unclear_audio_message: String,

    // ── STS pipeline mode ──────────────────────────────────────────────────────
    /// Which speech-to-speech pipeline to use. Controlled by `STS_MODE`.
    ///  - `STS_NORMAL`    → Groq Whisper STT → LLM Agent → OpenAI TTS  (default)
    ///  - `STS_STREAM`    → gpt-4o-realtime WebSocket (audio in/out + live transcript)
    pub sts_mode: StsMode,
    /// Realtime model name. `OPENAI_REALTIME_MODEL` env var.
    /// e.g. "gpt-4o-realtime-preview" or "gpt-4o-realtime-preview-2024-12-17"
    pub openai_realtime_model: String,
    /// WebSocket base URL. `OPENAI_REALTIME_WS_BASE` env var.
    /// Defaults to "wss://api.openai.com/v1/realtime".
    pub openai_realtime_ws_base: String,
    /// Full WSS URL override (optional). `OPENAI_REALTIME_URL` env var.
    /// When set it is used as-is; otherwise built as `ws_base?model=<model>`.
    pub openai_realtime_url: String,
    /// Realtime voice. `OPENAI_REALTIME_VOICE` env var.
    /// Options: alloy | echo | shimmer | verse | ash | ballad | coral.
    pub openai_realtime_voice: String,
    /// PCM16 sample rate sent to / received from the Realtime API.
    /// `OPENAI_REALTIME_SAMPLE_RATE` env var. Default: 24000.
    /// OpenAI Realtime currently only accepts 24000 Hz.
    pub openai_realtime_sample_rate: u32,
    /// LiveKit audio frames to batch before flushing to the WebSocket.
    /// `OPENAI_REALTIME_FLUSH_FRAMES` env var. Default: 4 (≈ 40 ms at 10 ms/frame).
    /// Lower = less latency; higher = less CPU overhead.
    pub openai_realtime_flush_frames: usize,
}

/// Selects the active Speech-to-Speech pipeline.
///
/// Set via the `STS_MODE` environment variable:
/// | Value           | Pipeline                                              |
/// |-----------------|-------------------------------------------------------|
/// | `STS_NORMAL`    | Legacy: Groq Whisper STT → LLM Agent → OpenAI TTS    |
/// | `STS_STREAM`    | gpt-4o-realtime single WebSocket (audio + text)       |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StsMode {
    /// Legacy pipeline (default when `STS_MODE` is unset or `STS_NORMAL`).
    #[default]
    Normal,
    /// Streaming gpt-4o-realtime WebSocket pipeline.
    Stream,
}

impl StsMode {
    pub fn from_env() -> Self {
        std::env::var("STS_MODE")
            .ok()
            .and_then(|value| Self::parse(&value).ok())
            .unwrap_or_default()
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_uppercase().as_str() {
            "STS_NORMAL" => Ok(Self::Normal),
            "STS_STREAM" => Ok(Self::Stream),
            other => Err(format!(
                "STS_MODE must be STS_NORMAL or STS_STREAM, got: {other}"
            )),
        }
    }

    pub fn is_realtime(self) -> bool {
        self == Self::Stream
    }
}

// AppConfig derives serde::Deserialize, so every field must implement it.
// StsMode stores no state in config files — it always reads from the env var
// at runtime — so we deserialize by ignoring the value and calling from_env().
impl<'de> serde::Deserialize<'de> for StsMode {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        // Accept any string (or missing field) and route through from_env().
        let _s: Option<String> = serde::Deserialize::deserialize(d).unwrap_or(None);
        Ok(StsMode::from_env())
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct TelephonyConfig {
    pub provider: TelephonyProvider,
    pub public_base_url: Option<String>,
    pub webhook_secret: Option<String>,
    pub twilio_account_sid: Option<String>,
    pub twilio_auth_token: Option<String>,
    pub telnyx_public_key: Option<String>,
    pub telnyx_api_key: Option<String>,
    pub telnyx_connection_id: Option<String>,
    pub vonage_signature_secret: Option<String>,
    pub vonage_api_key: Option<String>,
    pub vonage_api_secret: Option<String>,
    pub plivo_auth_id: Option<String>,
    pub plivo_auth_token: Option<String>,
    pub sip_gateway_url: Option<String>,
    pub sip_username: Option<String>,
    pub sip_password: Option<String>,
    pub custom_ingress_url: Option<String>,
    pub custom_api_key: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TelephonyProvider {
    Disabled,
    Twilio,
    Telnyx,
    Vonage,
    Plivo,
    Sip,
    Custom,
}

impl TelephonyProvider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Twilio => "twilio",
            Self::Telnyx => "telnyx",
            Self::Vonage => "vonage",
            Self::Plivo => "plivo",
            Self::Sip => "sip",
            Self::Custom => "custom",
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" | "disabled" | "none" => Ok(Self::Disabled),
            "twilio" => Ok(Self::Twilio),
            "telnyx" => Ok(Self::Telnyx),
            "vonage" => Ok(Self::Vonage),
            "plivo" => Ok(Self::Plivo),
            "sip" => Ok(Self::Sip),
            "custom" => Ok(Self::Custom),
            other => Err(format!("Unsupported TELEPHONY_PROVIDER: {other}")),
        }
    }
}

impl TelephonyConfig {
    fn optional_env(key: &str) -> Option<String> {
        env::var(key)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    }

    pub fn load() -> Result<Self, String> {
        let provider = TelephonyProvider::parse(
            &env::var("TELEPHONY_PROVIDER").unwrap_or_else(|_| "disabled".to_string()),
        )?;
        Ok(Self {
            provider,
            public_base_url: Self::optional_env("TELEPHONY_PUBLIC_BASE_URL"),
            webhook_secret: Self::optional_env("TELEPHONY_WEBHOOK_SECRET"),
            twilio_account_sid: Self::optional_env("TWILIO_ACCOUNT_SID"),
            twilio_auth_token: Self::optional_env("TWILIO_AUTH_TOKEN"),
            telnyx_public_key: Self::optional_env("TELNYX_PUBLIC_KEY"),
            telnyx_api_key: Self::optional_env("TELNYX_API_KEY"),
            telnyx_connection_id: Self::optional_env("TELNYX_CONNECTION_ID"),
            vonage_signature_secret: Self::optional_env("VONAGE_SIGNATURE_SECRET"),
            vonage_api_key: Self::optional_env("VONAGE_API_KEY"),
            vonage_api_secret: Self::optional_env("VONAGE_API_SECRET"),
            plivo_auth_id: Self::optional_env("PLIVO_AUTH_ID"),
            plivo_auth_token: Self::optional_env("PLIVO_AUTH_TOKEN"),
            sip_gateway_url: Self::optional_env("SIP_GATEWAY_URL"),
            sip_username: Self::optional_env("SIP_USERNAME"),
            sip_password: Self::optional_env("SIP_PASSWORD"),
            custom_ingress_url: Self::optional_env("CUSTOM_TELEPHONY_INGRESS_URL"),
            custom_api_key: Self::optional_env("CUSTOM_TELEPHONY_API_KEY"),
        })
    }

    pub fn provider_credentials_configured(&self) -> bool {
        match self.provider {
            TelephonyProvider::Disabled => false,
            TelephonyProvider::Twilio => {
                self.twilio_account_sid.is_some() && self.twilio_auth_token.is_some()
            }
            TelephonyProvider::Telnyx => {
                self.telnyx_api_key.is_some()
                    && self.telnyx_connection_id.is_some()
                    && self.telnyx_public_key.is_some()
            }
            TelephonyProvider::Vonage => {
                self.vonage_api_key.is_some() && self.vonage_signature_secret.is_some()
            }
            TelephonyProvider::Plivo => {
                self.plivo_auth_id.is_some() && self.plivo_auth_token.is_some()
            }
            TelephonyProvider::Sip => self.sip_gateway_url.is_some(),
            TelephonyProvider::Custom => self.custom_ingress_url.is_some(),
        }
    }

    pub fn media_ingress_configured(&self) -> bool {
        self.provider_credentials_configured()
            && self.public_base_url.is_some()
            && self.webhook_secret.is_some()
    }
}

impl AppConfig {
    pub fn load() -> Result<Self, String> {
        let mut cfg = Self::load_default();
        if env::var("PORT").is_ok() {
            cfg.port = parse_env("PORT")?;
        }
        if env::var("ORCHESTRATOR_PORT").is_ok() {
            cfg.orchestrator_port = parse_env("ORCHESTRATOR_PORT")?;
        }
        if env::var("AGENT_PORT").is_ok() {
            cfg.agent_port = parse_env("AGENT_PORT")?;
        }
        if env::var("VAD_SPEECH_PROBABILITY_THRESHOLD").is_ok() {
            cfg.vad_speech_probability_threshold = parse_env("VAD_SPEECH_PROBABILITY_THRESHOLD")?;
        }
        if env::var("VAD_BARGE_IN_PROBABILITY_THRESHOLD").is_ok() {
            cfg.vad_barge_in_probability_threshold =
                parse_env("VAD_BARGE_IN_PROBABILITY_THRESHOLD")?;
        }
        if env::var("VAD_MIN_SPEECH_AMPLITUDE").is_ok() {
            cfg.vad_min_speech_amplitude = parse_env("VAD_MIN_SPEECH_AMPLITUDE")?;
        }
        if env::var("VAD_MAX_SPEECH_AMPLITUDE").is_ok() {
            cfg.vad_max_speech_amplitude = parse_env("VAD_MAX_SPEECH_AMPLITUDE")?;
        }
        if env::var("VAD_ENERGY_BYPASS_AMPLITUDE").is_ok() {
            cfg.vad_energy_bypass_amplitude = parse_env("VAD_ENERGY_BYPASS_AMPLITUDE")?;
        }
        if env::var("VAD_BARGE_IN_AMPLITUDE").is_ok() {
            cfg.vad_barge_in_amplitude = parse_env("VAD_BARGE_IN_AMPLITUDE")?;
        }
        if env::var("VAD_BARGE_IN_MIN_CHUNKS").is_ok() {
            cfg.vad_barge_in_min_chunks = parse_env("VAD_BARGE_IN_MIN_CHUNKS")?;
        }
        if env::var("VAD_END_OF_SPEECH_TIMEOUT_MS").is_ok() {
            cfg.vad_end_of_speech_timeout_ms = parse_env("VAD_END_OF_SPEECH_TIMEOUT_MS")?;
        }
        if env::var("VAD_ANALYSIS_WINDOW_MS").is_ok() {
            cfg.vad_analysis_window_ms = parse_env("VAD_ANALYSIS_WINDOW_MS")?;
        }
        if env::var("VAD_MIN_SPEECH_CHUNKS_IN_WINDOW").is_ok() {
            cfg.vad_min_speech_chunks_in_window = parse_env("VAD_MIN_SPEECH_CHUNKS_IN_WINDOW")?;
        }
        if env::var("VAD_CLIP_LEVEL").is_ok() {
            cfg.vad_clip_level = parse_env("VAD_CLIP_LEVEL")?;
        }
        if env::var("VAD_MAX_CREST_FACTOR").is_ok() {
            cfg.vad_max_crest_factor = parse_env("VAD_MAX_CREST_FACTOR")?;
        }
        if env::var("VAD_MIN_ENERGY_VARIATION").is_ok() {
            cfg.vad_min_energy_variation = parse_env("VAD_MIN_ENERGY_VARIATION")?;
        }
        if env::var("VAD_MAX_LOUD_RATIO").is_ok() {
            cfg.vad_max_loud_ratio = parse_env("VAD_MAX_LOUD_RATIO")?;
        }
        if env::var("STS_TEXT_JITTER_MS").is_ok() {
            cfg.sts_text_jitter_ms = parse_env("STS_TEXT_JITTER_MS")?;
        }
        if env::var("STS_TEXT_MAX_WAIT_MS").is_ok() {
            cfg.sts_text_max_wait_ms = parse_env("STS_TEXT_MAX_WAIT_MS")?;
        }
        if env::var("GRAPH_CONTEXT_REQUEST_TIMEOUT_MS").is_ok() {
            cfg.graph_context_request_timeout_ms = parse_env("GRAPH_CONTEXT_REQUEST_TIMEOUT_MS")?;
        }
        if env::var("ORCHESTRATOR_TOOL_TIMEOUT_MS").is_ok() {
            cfg.orchestrator_tool_timeout_ms = parse_env("ORCHESTRATOR_TOOL_TIMEOUT_MS")?;
        }
        if env::var("ORCHESTRATOR_TOOL_MAX_RETRIES").is_ok() {
            cfg.orchestrator_tool_max_retries = parse_env("ORCHESTRATOR_TOOL_MAX_RETRIES")?;
        }
        if env::var("ORCHESTRATOR_RETRY_BASE_MS").is_ok() {
            cfg.orchestrator_retry_base_ms = parse_env("ORCHESTRATOR_RETRY_BASE_MS")?;
        }
        if env::var("ORCHESTRATOR_TASK_LEASE_MS").is_ok() {
            cfg.orchestrator_task_lease_ms = parse_env("ORCHESTRATOR_TASK_LEASE_MS")?;
        }
        if env::var("STS_CONTEXT_UPDATE_ACK").is_ok() {
            cfg.sts_context_update_ack = env::var("STS_CONTEXT_UPDATE_ACK").unwrap_or_default();
        }
        if env::var("RECORDING_MAX_BUFFER_BYTES").is_ok() {
            cfg.recording_max_buffer_bytes = parse_env("RECORDING_MAX_BUFFER_BYTES")?;
        }
        if env::var("STS_MODE").is_ok() {
            cfg.sts_mode = StsMode::parse(&parse_env::<String>("STS_MODE")?)?;
        }
        cfg.telephony = TelephonyConfig::load()?;
        if !(200..=3_000).contains(&cfg.vad_end_of_speech_timeout_ms) {
            return Err("VAD_END_OF_SPEECH_TIMEOUT_MS must be between 200 and 3000".into());
        }
        if !(1_000..=60_000).contains(&cfg.orchestrator_tool_timeout_ms) {
            return Err("ORCHESTRATOR_TOOL_TIMEOUT_MS must be between 1000 and 60000".into());
        }
        if cfg.recording_max_buffer_bytes == 0 {
            return Err("RECORDING_MAX_BUFFER_BYTES must be greater than zero".into());
        }
        Ok(cfg)
    }

    /// String context and credentials come only from the environment. Missing strings are empty.
    pub fn load_default() -> Self {
        let _ = dotenvy::dotenv();
        Self {
            port: env::var("PORT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(4000),
            orchestrator_port: env::var("ORCHESTRATOR_PORT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(4001),
            agent_port: env::var("AGENT_PORT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(4002),
            redis_url: env::var("REDIS_URL").unwrap_or_default(),
            // Application services use Neon’s pooled endpoint.  A direct endpoint is
            // reserved for schema migration tooling, not request/worker traffic.
            postgres_url: env::var("NEON_POOLER_URL")
                .or_else(|_| env::var("POSTGRES_URL"))
                .or_else(|_| env::var("DATABASE_URL"))
                .unwrap_or_default(),
            livekit_url: env::var("LIVEKIT_URL").unwrap_or_default(),
            livekit_api_key: env::var("LIVEKIT_API_KEY").unwrap_or_default(),
            livekit_api_secret: env::var("LIVEKIT_API_SECRET").unwrap_or_default(),
            stt_provider: env::var("STT_PROVIDER").unwrap_or_default(),
            llm_provider: env::var("LLM_PROVIDER").unwrap_or_default(),
            tts_provider: env::var("TTS_PROVIDER").unwrap_or_default(),
            openai_api_key: env::var("OPENAI_API_KEY").unwrap_or_default(),
            groq_api_key: env::var("GROQ_API_KEY").unwrap_or_default(),
            elevenlabs_api_key: env::var("ELEVENLABS_API_KEY").unwrap_or_default(),
            groq_stt_url: env::var("GROQ_STT_URL").unwrap_or_default(),
            stt_model: env::var("STT_MODEL")
                .or_else(|_| env::var("GROQ_STT_MODEL"))
                .unwrap_or_default(),
            openai_tts_url: env::var("OPENAI_TTS_URL").unwrap_or_default(),
            r2_bucket_name: env::var("R2_BUCKET_NAME").unwrap_or_default(),
            r2_account_id: env::var("R2_ACCOUNT_ID").unwrap_or_default(),
            r2_access_key_id: env::var("R2_ACCESS_KEY_ID").unwrap_or_default(),
            r2_secret_access_key: env::var("R2_SECRET_ACCESS_KEY").unwrap_or_default(),
            r2_endpoint_url: env::var("R2_ENDPOINT_URL").unwrap_or_default(),
            tone_model_api_key: env::var("TONE_MODEL_API_KEY").unwrap_or_default(),
            upstash_vector_rest_url: env::var("UPSTASH_VECTOR_REST_URL")
                .or_else(|_| env::var("UPSTASH_VECTOR_URL"))
                .unwrap_or_default(),
            upstash_vector_rest_token: env::var("UPSTASH_VECTOR_REST_TOKEN")
                .or_else(|_| env::var("UPSTASH_VECTOR_TOKEN"))
                .unwrap_or_default(),
            node_env: env::var("NODE_ENV").unwrap_or_default(),
            jwt_secret: env::var("JWT_SECRET").unwrap_or_default(),
            neo4j_uri: env::var("NEO4J_URI")
                .or_else(|_| env::var("MEMGRAPH_URI"))
                .ok()
                .filter(|value| !value.trim().is_empty()),
            neo4j_username: env::var("NEO4J_USERNAME")
                .or_else(|_| env::var("MEMGRAPH_USERNAME"))
                .ok()
                .filter(|value| !value.trim().is_empty()),
            neo4j_password: env::var("NEO4J_PASSWORD")
                .or_else(|_| env::var("MEMGRAPH_PASSWORD"))
                .ok()
                .filter(|value| !value.trim().is_empty()),
            neo4j_database: env::var("NEO4J_DATABASE")
                .ok()
                .filter(|value| !value.trim().is_empty()),
            app_domain_name: env::var("APP_DOMAIN_NAME").unwrap_or_default(),
            app_currency: env::var("APP_CURRENCY").unwrap_or_default(),
            kyc_document_name: env::var("KYC_DOCUMENT_NAME").unwrap_or_default(),
            kyc_required_for_booking: env::var("KYC_REQUIRED_FOR_BOOKING")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(true),
            kyc_unverified_message: env::var("KYC_UNVERIFIED_MESSAGE").unwrap_or_default(),
            domain_fallback_message: env::var("DOMAIN_FALLBACK_MESSAGE").unwrap_or_default(),
            filter_primary_type_a: env::var("FILTER_PRIMARY_TYPE_A").unwrap_or_default(),
            filter_primary_type_b: env::var("FILTER_PRIMARY_TYPE_B").unwrap_or_default(),
            filter_type_a_synonyms: env::var("FILTER_TYPE_A_SYNONYMS")
                .unwrap_or_default()
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_lowercase)
                .collect(),
            filter_type_b_synonyms: env::var("FILTER_TYPE_B_SYNONYMS")
                .unwrap_or_default()
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_lowercase)
                .collect(),
            filter_feature_1_synonyms: env::var("FILTER_FEATURE_1_SYNONYMS")
                .unwrap_or_default()
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_lowercase)
                .collect(),
            filter_feature_2_synonyms: env::var("FILTER_FEATURE_2_SYNONYMS")
                .unwrap_or_default()
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_lowercase)
                .collect(),
            action_booking_keywords: env::var("ACTION_BOOKING_KEYWORDS")
                .unwrap_or_default()
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_lowercase)
                .collect(),
            contextual_follow_up_phrases: env::var("CONTEXTUAL_FOLLOW_UP_PHRASES")
                .unwrap_or_default()
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_lowercase)
                .collect(),
            contextual_follow_up_clarification: env::var("CONTEXTUAL_FOLLOW_UP_CLARIFICATION")
                .unwrap_or_default(),
            llm_api_key: env::var("LLM_API_KEY")
                .or_else(|_| env::var("AGENT_LLM_API_KEY"))
                .or_else(|_| env::var("OPENAI_API_KEY"))
                .unwrap_or_default(),
            llm_endpoint_url: env::var("LLM_ENDPOINT_URL").unwrap_or_default(),
            llm_model: env::var("LLM_MODEL")
                .or_else(|_| env::var("AGENT_LLM_MODEL"))
                .unwrap_or_default(),
            agent_system_prompt: env::var("AGENT_SYSTEM_PROMPT").unwrap_or_default(),
            agent_context_prompt_template: env::var("AGENT_CONTEXT_PROMPT_TEMPLATE")
                .unwrap_or_default(),
            agent_edge_prompt_template: env::var("AGENT_EDGE_PROMPT_TEMPLATE").unwrap_or_default(),
            agent_turn_planner_prompt_template: env::var("AGENT_TURN_PLANNER_PROMPT_TEMPLATE")
                .unwrap_or_default(),
            agent_turn_router_prompt_template: env::var("AGENT_TURN_ROUTER_PROMPT_TEMPLATE")
                .unwrap_or_default(),
            agent_confirmation_planner_prompt_template: env::var(
                "AGENT_CONFIRMATION_PLANNER_PROMPT_TEMPLATE",
            )
            .unwrap_or_default(),
            agent_reference_planner_prompt_template: env::var(
                "AGENT_REFERENCE_PLANNER_PROMPT_TEMPLATE",
            )
            .unwrap_or_default(),
            agent_dependency_planner_prompt_template: env::var(
                "AGENT_DEPENDENCY_PLANNER_PROMPT_TEMPLATE",
            )
            .unwrap_or_default(),
            agent_tool_result_prompt_template: env::var("AGENT_TOOL_RESULT_PROMPT_TEMPLATE")
                .unwrap_or_default(),
            agent_graph_enrichment_prompt_template: env::var(
                "AGENT_GRAPH_ENRICHMENT_PROMPT_TEMPLATE",
            )
            .unwrap_or_default(),
            tool_wait_tts_prompt: env::var("TOOL_WAIT_TTS_PROMPT").unwrap_or_default(),
            tool_retry_tts_prompt: env::var("TOOL_RETRY_TTS_PROMPT").unwrap_or_else(|_| {
                "It's taking a little longer than expected. I'm still checking that for you."
                    .to_string()
            }),
            sts_context_update_ack: env::var("STS_CONTEXT_UPDATE_ACK").unwrap_or_else(|_| {
                "I've gathered a bit more context. Give me a moment while I put that together."
                    .to_string()
            }),
            stt_prompt: env::var("STT_PROMPT").unwrap_or_default(),
            stt_language: env::var("STT_LANGUAGE").unwrap_or_else(|_| "en".to_string()),
            llm_response_language: env::var("LLM_RESPONSE_LANGUAGE").unwrap_or_default(),
            vad_speech_probability_threshold: env::var("VAD_SPEECH_PROBABILITY_THRESHOLD")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(0.15),
            vad_barge_in_probability_threshold: env::var("VAD_BARGE_IN_PROBABILITY_THRESHOLD")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(0.40),
            vad_min_speech_amplitude: env::var("VAD_MIN_SPEECH_AMPLITUDE")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(100),
            vad_max_speech_amplitude: env::var("VAD_MAX_SPEECH_AMPLITUDE")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(20_000),
            vad_energy_bypass_amplitude: env::var("VAD_ENERGY_BYPASS_AMPLITUDE")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(100),
            vad_barge_in_amplitude: env::var("VAD_BARGE_IN_AMPLITUDE")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(100),
            vad_barge_in_min_chunks: env::var("VAD_BARGE_IN_MIN_CHUNKS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(2),
            vad_end_of_speech_timeout_ms: env::var("VAD_END_OF_SPEECH_TIMEOUT_MS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(650),
            vad_analysis_window_ms: env::var("VAD_ANALYSIS_WINDOW_MS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(1200),
            vad_min_speech_chunks_in_window: env::var("VAD_MIN_SPEECH_CHUNKS_IN_WINDOW")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(4),
            vad_clip_level: env::var("VAD_CLIP_LEVEL")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(30_000),
            vad_max_crest_factor: env::var("VAD_MAX_CREST_FACTOR")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(8.0),
            vad_min_energy_variation: env::var("VAD_MIN_ENERGY_VARIATION")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(0.25),
            vad_max_loud_ratio: env::var("VAD_MAX_LOUD_RATIO")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(0.6),
            sts_text_jitter_ms: env::var("STS_TEXT_JITTER_MS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(120),
            sts_text_max_wait_ms: env::var("STS_TEXT_MAX_WAIT_MS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(1800),
            graph_context_request_timeout_ms: env::var("GRAPH_CONTEXT_REQUEST_TIMEOUT_MS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(500),
            orchestrator_tool_timeout_ms: env::var("ORCHESTRATOR_TOOL_TIMEOUT_MS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(15_000),
            orchestrator_tool_max_retries: env::var("ORCHESTRATOR_TOOL_MAX_RETRIES")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(3),
            orchestrator_retry_base_ms: env::var("ORCHESTRATOR_RETRY_BASE_MS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(1_000),
            orchestrator_task_lease_ms: env::var("ORCHESTRATOR_TASK_LEASE_MS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(30_000),
            decision_llm_api_key: env::var("DECISION_LLM_API_KEY")
                .or_else(|_| env::var("LLM_API_KEY"))
                .or_else(|_| env::var("AGENT_LLM_API_KEY"))
                .or_else(|_| env::var("OPENAI_API_KEY"))
                .unwrap_or_default(),
            decision_llm_endpoint_url: env::var("DECISION_LLM_ENDPOINT_URL")
                .or_else(|_| env::var("LLM_ENDPOINT_URL"))
                .unwrap_or_default(),
            decision_llm_model: env::var("DECISION_LLM_MODEL")
                .or_else(|_| env::var("DECISION_MODEL"))
                .unwrap_or_default(),
            fast_model_api_key: env::var("FAST_MODEL_API_KEY")
                .or_else(|_| env::var("LLM_API_KEY"))
                .or_else(|_| env::var("AGENT_LLM_API_KEY"))
                .or_else(|_| env::var("OPENAI_API_KEY"))
                .unwrap_or_default(),
            fast_model_endpoint_url: env::var("FAST_MODEL_ENDPOINT_URL")
                .or_else(|_| env::var("LLM_ENDPOINT_URL"))
                .unwrap_or_default(),
            fast_model: env::var("FAST_MODEL")
                .or_else(|_| env::var("DECISION_LLM_MODEL"))
                .or_else(|_| env::var("LLM_MODEL"))
                .unwrap_or_default(),
            rag_business_keywords: env::var("RAG_BUSINESS_KEYWORDS")
                .unwrap_or_default()
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect(),
            tts_model: env::var("TTS_MODEL").unwrap_or_default(),
            tts_fallback_model: env::var("TTS_FALLBACK_MODEL").unwrap_or_default(),
            tts_voice: env::var("TTS_VOICE").unwrap_or_default(),
            recording_max_buffer_bytes: env::var("RECORDING_MAX_BUFFER_BYTES")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(33_554_432),
            unrelated_query_message: env::var("UNRELATED_QUERY_MESSAGE").unwrap_or_default(),
            property_match_message_template: env::var("PROPERTY_MATCH_MESSAGE_TEMPLATE")
                .unwrap_or_default(),
            rag_search_success_message: env::var("RAG_SEARCH_SUCCESS_MESSAGE").unwrap_or_default(),
            contract_not_found_message: env::var("CONTRACT_NOT_FOUND_MESSAGE").unwrap_or_default(),
            unclear_audio_message: env::var("UNCLEAR_AUDIO_MESSAGE").unwrap_or_else(|_| {
                "I am not able to hear you properly, please repeat.".to_string()
            }),
            telephony: TelephonyConfig::load().unwrap_or_else(|_| disabled_telephony_config()),
            sts_mode: StsMode::from_env(),
            openai_realtime_model: env::var("OPENAI_REALTIME_MODEL")
                .unwrap_or_else(|_| "gpt-4o-realtime-preview".to_string()),
            openai_realtime_ws_base: env::var("OPENAI_REALTIME_WS_BASE")
                .unwrap_or_else(|_| "wss://api.openai.com/v1/realtime".to_string()),
            openai_realtime_url: env::var("OPENAI_REALTIME_URL").unwrap_or_default(),
            openai_realtime_voice: env::var("OPENAI_REALTIME_VOICE")
                .unwrap_or_else(|_| "shimmer".to_string()),
            openai_realtime_sample_rate: env::var("OPENAI_REALTIME_SAMPLE_RATE")
                .ok()
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(24_000),
            openai_realtime_flush_frames: env::var("OPENAI_REALTIME_FLUSH_FRAMES")
                .ok()
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(4),
        }
    }
}

fn disabled_telephony_config() -> TelephonyConfig {
    TelephonyConfig {
        provider: TelephonyProvider::Disabled,
        public_base_url: None,
        webhook_secret: None,
        twilio_account_sid: None,
        twilio_auth_token: None,
        telnyx_public_key: None,
        telnyx_api_key: None,
        telnyx_connection_id: None,
        vonage_signature_secret: None,
        vonage_api_key: None,
        vonage_api_secret: None,
        plivo_auth_id: None,
        plivo_auth_token: None,
        sip_gateway_url: None,
        sip_username: None,
        sip_password: None,
        custom_ingress_url: None,
        custom_api_key: None,
    }
}

#[cfg(test)]
mod environment_tests {
    #[test]
    fn missing_credentials_and_context_are_empty() {
        const CHILD: &str = "ESTATE_EMPTY_CONFIG_TEST_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .env_clear()
                .env(CHILD, "1")
                .current_dir(std::env::temp_dir())
                .args([
                    "--exact",
                    "environment_tests::missing_credentials_and_context_are_empty",
                    "--nocapture",
                ])
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            return;
        }
        let cfg = super::AppConfig::load().unwrap();
        for value in [
            cfg.jwt_secret,
            cfg.postgres_url,
            cfg.redis_url,
            cfg.llm_api_key,
            cfg.llm_model,
            cfg.agent_system_prompt,
            cfg.domain_fallback_message,
            cfg.r2_endpoint_url,
            cfg.app_domain_name,
        ] {
            assert!(
                value.is_empty(),
                "Missing values must not acquire hardcoded defaults"
            );
        }
        assert!(cfg.action_booking_keywords.is_empty());
        assert!(cfg.filter_type_a_synonyms.is_empty());
    }
}
