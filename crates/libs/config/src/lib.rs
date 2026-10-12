use serde::Deserialize;
use std::env;
use std::path::Path;
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
    pub admin_email: String,
    pub admin_password: String,
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

    // Pluggable LLM Model & Endpoint
    pub llm_api_key: String,
    pub llm_endpoint_url: String,
    pub llm_model: String,
    pub agent_system_prompt: String,
    pub agent_context_prompt_template: String,
    pub agent_edge_prompt_template: String,
    pub stt_prompt: String,
    pub vad_speech_probability_threshold: f32,
    pub vad_barge_in_probability_threshold: f32,
    pub vad_min_speech_amplitude: u16,
    pub vad_energy_bypass_amplitude: u16,
    pub vad_barge_in_amplitude: u16,
    pub vad_end_of_speech_timeout_ms: u64,
    pub decision_llm_api_key: String,
    pub decision_llm_endpoint_url: String,
    pub decision_llm_model: String,
    pub tts_model: String,
    pub tts_fallback_model: String,
    pub tts_voice: String,
    pub recording_max_buffer_bytes: usize,

    // Dynamic Server Text Messages (Zero Hardcoding Rule)
    pub unrelated_query_message: String,
    pub property_match_message_template: String,
    pub rag_search_success_message: String,
    pub contract_not_found_message: String,
}

impl AppConfig {
    pub fn load() -> Result<Self, String> {
        if Path::new(".env").exists() {
            let _ = dotenvy::dotenv();
        } else if Path::new("../.env").exists() {
            let _ = dotenvy::from_path("../.env");
        } else if Path::new("../../.env").exists() {
            let _ = dotenvy::from_path("../../.env");
        }

        let port = env::var("PORT")
            .unwrap_or_else(|_| "4000".to_string())
            .parse::<u16>()
            .map_err(|e| format!("Invalid PORT: {}", e))?;

        let orchestrator_port = env::var("ORCHESTRATOR_PORT")
            .unwrap_or_else(|_| "4001".to_string())
            .parse::<u16>()
            .map_err(|e| format!("Invalid ORCHESTRATOR_PORT: {}", e))?;

        let agent_port = env::var("AGENT_PORT")
            .unwrap_or_else(|_| "4002".to_string())
            .parse::<u16>()
            .map_err(|e| format!("Invalid AGENT_PORT: {}", e))?;

        let redis_url =
            env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());
        let postgres_url = env::var("POSTGRES_URL")
            .or_else(|_| env::var("DATABASE_URL"))
            .unwrap_or_else(|_| {
                "postgresql://postgres:postgres@localhost:5432/uae_real_estate".to_string()
            });

        let livekit_url = env::var("LIVEKIT_URL")
            .map_err(|_| "LIVEKIT_URL not set in environment".to_string())?;
        let livekit_api_key = env::var("LIVEKIT_API_KEY")
            .map_err(|_| "LIVEKIT_API_KEY not set in environment".to_string())?;
        let livekit_api_secret = env::var("LIVEKIT_API_SECRET")
            .map_err(|_| "LIVEKIT_API_SECRET not set in environment".to_string())?;

        let stt_provider = env::var("STT_PROVIDER").unwrap_or_else(|_| "openai".to_string());
        let llm_provider = env::var("LLM_PROVIDER").unwrap_or_else(|_| "openai".to_string());
        let tts_provider = env::var("TTS_PROVIDER").unwrap_or_else(|_| "openai".to_string());

        let openai_api_key = env::var("OPENAI_API_KEY")
            .map_err(|_| "OPENAI_API_KEY not set in environment".to_string())?;
        let groq_api_key = env::var("GROQ_API_KEY").unwrap_or_else(|_| "".to_string());
        let elevenlabs_api_key = env::var("ELEVENLABS_API_KEY").unwrap_or_else(|_| "".to_string());

        let r2_bucket_name = env::var("R2_BUCKET_NAME")
            .map_err(|_| "R2_BUCKET_NAME not set in environment".to_string())?;
        let r2_account_id = env::var("R2_ACCOUNT_ID")
            .map_err(|_| "R2_ACCOUNT_ID not set in environment".to_string())?;
        let r2_access_key_id = env::var("R2_ACCESS_KEY_ID")
            .map_err(|_| "R2_ACCESS_KEY_ID not set in environment".to_string())?;
        let r2_secret_access_key = env::var("R2_SECRET_ACCESS_KEY")
            .map_err(|_| "R2_SECRET_ACCESS_KEY not set in environment".to_string())?;
        let r2_endpoint_url = env::var("R2_ENDPOINT_URL")
            .map_err(|_| "R2_ENDPOINT_URL not set in environment".to_string())?;
        let tone_model_api_key = env::var("TONE_MODEL_API_KEY").unwrap_or_default();
        let upstash_vector_rest_url = env::var("UPSTASH_VECTOR_REST_URL")
            .or_else(|_| env::var("UPSTASH_VECTOR_URL"))
            .map_err(|_| "UPSTASH_VECTOR_REST_URL not set in environment".to_string())?;
        let upstash_vector_rest_token = env::var("UPSTASH_VECTOR_REST_TOKEN")
            .or_else(|_| env::var("UPSTASH_VECTOR_TOKEN"))
            .map_err(|_| "UPSTASH_VECTOR_REST_TOKEN not set in environment".to_string())?;

        let groq_stt_url = env::var("GROQ_STT_URL")
            .unwrap_or_else(|_| "https://api.groq.com/openai/v1/audio/transcriptions".to_string());
        let stt_model = env::var("STT_MODEL")
            .or_else(|_| env::var("GROQ_STT_MODEL"))
            .unwrap_or_else(|_| "whisper-large-v3".to_string());
        let openai_tts_url = env::var("OPENAI_TTS_URL")
            .unwrap_or_else(|_| "https://api.openai.com/v1/audio/speech".to_string());

        let node_env = env::var("NODE_ENV").unwrap_or_else(|_| "development".to_string());
        let admin_email =
            env::var("ADMIN_EMAIL").unwrap_or_else(|_| "admin@uae-estate.ae".to_string());
        let admin_password = env::var("ADMIN_PASSWORD").unwrap_or_else(|_| "admin".to_string());
        let jwt_secret = env::var("JWT_SECRET").map_err(|_| {
            "JWT_SECRET not set in environment. Must be imported from .env".to_string()
        })?;

        // Graph DB
        let neo4j_uri = env::var("NEO4J_URI")
            .or_else(|_| env::var("MEMGRAPH_URI"))
            .ok();
        let neo4j_username = env::var("NEO4J_USERNAME")
            .or_else(|_| env::var("MEMGRAPH_USERNAME"))
            .ok();
        let neo4j_password = env::var("NEO4J_PASSWORD")
            .or_else(|_| env::var("MEMGRAPH_PASSWORD"))
            .ok();
        let neo4j_database = env::var("NEO4J_DATABASE").ok();

        // General Domain & Rules 14 and 15
        let app_domain_name =
            env::var("APP_DOMAIN_NAME").unwrap_or_else(|_| "Real Estate".to_string());
        let app_currency = env::var("APP_CURRENCY").unwrap_or_else(|_| "AED".to_string());
        let kyc_document_name =
            env::var("KYC_DOCUMENT_NAME").unwrap_or_else(|_| "Emirates ID".to_string());
        let kyc_required_for_booking = env::var("KYC_REQUIRED_FOR_BOOKING")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(true);
        let kyc_unverified_message = env::var("KYC_UNVERIFIED_MESSAGE").unwrap_or_else(|_| {
            "Viewing slot reservation requires verified {document_name} documents. Please upload your {document_name} in your user profile to unlock slot booking. In the meantime, I can assist you with recommendations, virtual tours, and price comparisons.".to_string()
        });
        let domain_fallback_message = env::var("DOMAIN_FALLBACK_MESSAGE").unwrap_or_else(|_| {
            "That detail is not currently available in the property information I retrieved. You can ask about another available property, its price or features, or another UAE real estate topic.".to_string()
        });

        // Dynamic Filter Synonyms from env
        let filter_primary_type_a =
            env::var("FILTER_PRIMARY_TYPE_A").unwrap_or_else(|_| "buy".to_string());
        let filter_primary_type_b =
            env::var("FILTER_PRIMARY_TYPE_B").unwrap_or_else(|_| "rent".to_string());
        let filter_type_a_synonyms = env::var("FILTER_TYPE_A_SYNONYMS")
            .unwrap_or_else(|_| "buy,sale,purchase,acquire".to_string())
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .collect();
        let filter_type_b_synonyms = env::var("FILTER_TYPE_B_SYNONYMS")
            .unwrap_or_else(|_| "rent,lease,leasing,annual".to_string())
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .collect();
        let filter_feature_1_synonyms = env::var("FILTER_FEATURE_1_SYNONYMS")
            .unwrap_or_else(|_| "pet,dog,cat,animal".to_string())
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .collect();
        let filter_feature_2_synonyms = env::var("FILTER_FEATURE_2_SYNONYMS")
            .unwrap_or_else(|_| "metro,commute,station,subway,train,transit".to_string())
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .collect();
        let action_booking_keywords = env::var("ACTION_BOOKING_KEYWORDS")
            .unwrap_or_else(|_| "book,reserve,schedule,viewing,slot,appointment".to_string())
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .collect();

        // Pluggable LLM endpoints
        let llm_api_key = env::var("LLM_API_KEY")
            .or_else(|_| env::var("AGENT_LLM_API_KEY"))
            .unwrap_or_else(|_| openai_api_key.clone());
        let llm_endpoint_url = env::var("LLM_ENDPOINT_URL")
            .unwrap_or_else(|_| "https://api.openai.com/v1/chat/completions".to_string());
        let llm_model = env::var("LLM_MODEL")
            .or_else(|_| env::var("AGENT_LLM_MODEL"))
            .unwrap_or_else(|_| "gpt-4o-mini".to_string());
        let agent_system_prompt = env::var("AGENT_SYSTEM_PROMPT")
            .map_err(|_| "AGENT_SYSTEM_PROMPT not set in environment".to_string())?;
        let agent_context_prompt_template = env::var("AGENT_CONTEXT_PROMPT_TEMPLATE")
            .map_err(|_| "AGENT_CONTEXT_PROMPT_TEMPLATE not set in environment".to_string())?;
        let agent_edge_prompt_template = env::var("AGENT_EDGE_PROMPT_TEMPLATE")
            .map_err(|_| "AGENT_EDGE_PROMPT_TEMPLATE not set in environment".to_string())?;
        let stt_prompt =
            env::var("STT_PROMPT").map_err(|_| "STT_PROMPT not set in environment".to_string())?;
        let vad_speech_probability_threshold = parse_env("VAD_SPEECH_PROBABILITY_THRESHOLD")?;
        let vad_barge_in_probability_threshold = parse_env("VAD_BARGE_IN_PROBABILITY_THRESHOLD")?;
        let vad_min_speech_amplitude = parse_env("VAD_MIN_SPEECH_AMPLITUDE")?;
        let vad_energy_bypass_amplitude = parse_env("VAD_ENERGY_BYPASS_AMPLITUDE")?;
        let vad_barge_in_amplitude = parse_env("VAD_BARGE_IN_AMPLITUDE")?;
        let vad_end_of_speech_timeout_ms = parse_env("VAD_END_OF_SPEECH_TIMEOUT_MS")?;
        if !(200..=3_000).contains(&vad_end_of_speech_timeout_ms) {
            return Err("VAD_END_OF_SPEECH_TIMEOUT_MS must be between 200 and 3000".to_string());
        }
        let decision_llm_api_key =
            env::var("DECISION_LLM_API_KEY").unwrap_or_else(|_| llm_api_key.clone());
        let decision_llm_endpoint_url =
            env::var("DECISION_LLM_ENDPOINT_URL").unwrap_or_else(|_| llm_endpoint_url.clone());
        let decision_llm_model = env::var("DECISION_LLM_MODEL")
            .or_else(|_| env::var("DECISION_MODEL"))
            .map_err(|_| "DECISION_LLM_MODEL not set in environment".to_string())?;
        let tts_model =
            env::var("TTS_MODEL").map_err(|_| "TTS_MODEL not set in environment".to_string())?;
        let tts_fallback_model = env::var("TTS_FALLBACK_MODEL")
            .map_err(|_| "TTS_FALLBACK_MODEL not set in environment".to_string())?;
        let tts_voice =
            env::var("TTS_VOICE").map_err(|_| "TTS_VOICE not set in environment".to_string())?;
        let recording_max_buffer_bytes = parse_env("RECORDING_MAX_BUFFER_BYTES")?;
        if recording_max_buffer_bytes == 0 {
            return Err("RECORDING_MAX_BUFFER_BYTES must be greater than zero".to_string());
        }

        let unrelated_query_message = env::var("UNRELATED_QUERY_MESSAGE")
            .map_err(|_| "UNRELATED_QUERY_MESSAGE not set in environment".to_string())?;
        let property_match_message_template = env::var("PROPERTY_MATCH_MESSAGE_TEMPLATE")
            .map_err(|_| "PROPERTY_MATCH_MESSAGE_TEMPLATE not set in environment".to_string())?;
        let rag_search_success_message = env::var("RAG_SEARCH_SUCCESS_MESSAGE")
            .map_err(|_| "RAG_SEARCH_SUCCESS_MESSAGE not set in environment".to_string())?;
        let contract_not_found_message = env::var("CONTRACT_NOT_FOUND_MESSAGE")
            .map_err(|_| "CONTRACT_NOT_FOUND_MESSAGE not set in environment".to_string())?;

        Ok(AppConfig {
            port,
            orchestrator_port,
            agent_port,
            redis_url,
            postgres_url,
            livekit_url,
            livekit_api_key,
            livekit_api_secret,
            stt_provider,
            llm_provider,
            tts_provider,
            openai_api_key,
            groq_api_key,
            elevenlabs_api_key,
            groq_stt_url,
            stt_model,
            openai_tts_url,
            r2_bucket_name,
            r2_account_id,
            r2_access_key_id,
            r2_secret_access_key,
            r2_endpoint_url,
            tone_model_api_key,
            upstash_vector_rest_url,
            upstash_vector_rest_token,
            node_env,
            admin_email,
            admin_password,
            jwt_secret,
            neo4j_uri,
            neo4j_username,
            neo4j_password,
            neo4j_database,
            app_domain_name,
            app_currency,
            kyc_document_name,
            kyc_required_for_booking,
            kyc_unverified_message,
            domain_fallback_message,
            filter_primary_type_a,
            filter_primary_type_b,
            filter_type_a_synonyms,
            filter_type_b_synonyms,
            filter_feature_1_synonyms,
            filter_feature_2_synonyms,
            action_booking_keywords,
            llm_api_key,
            llm_endpoint_url,
            llm_model,
            agent_system_prompt,
            agent_context_prompt_template,
            agent_edge_prompt_template,
            stt_prompt,
            vad_speech_probability_threshold,
            vad_barge_in_probability_threshold,
            vad_min_speech_amplitude,
            vad_energy_bypass_amplitude,
            vad_barge_in_amplitude,
            vad_end_of_speech_timeout_ms,
            decision_llm_api_key,
            decision_llm_endpoint_url,
            decision_llm_model,
            tts_model,
            tts_fallback_model,
            tts_voice,
            recording_max_buffer_bytes,
            unrelated_query_message,
            property_match_message_template,
            rag_search_success_message,
            contract_not_found_message,
        })
    }

    pub fn load_default() -> Self {
        if Path::new(".env").exists() {
            let _ = dotenvy::dotenv();
        } else if Path::new("../.env").exists() {
            let _ = dotenvy::from_path("../.env");
        } else if Path::new("../../.env").exists() {
            let _ = dotenvy::from_path("../../.env");
        }

        Self {
            port: 4000,
            orchestrator_port: 4001,
            agent_port: 4002,
            redis_url: "redis://localhost:6379".to_string(),
            postgres_url: "postgresql://postgres:postgres@localhost:5432/uae_real_estate".to_string(),
            livekit_url: env::var("LIVEKIT_URL").unwrap_or_default(),
            livekit_api_key: env::var("LIVEKIT_API_KEY").unwrap_or_default(),
            livekit_api_secret: env::var("LIVEKIT_API_SECRET").unwrap_or_default(),
            stt_provider: "openai".to_string(),
            llm_provider: "openai".to_string(),
            tts_provider: "openai".to_string(),
            openai_api_key: "sk-proj-mock-key".to_string(),
            groq_api_key: "gsk_mock_groq_key".to_string(),
            elevenlabs_api_key: "mock_elevenlabs_key".to_string(),
            groq_stt_url: "https://api.groq.com/openai/v1/audio/transcriptions".to_string(),
            stt_model: "whisper-large-v3".to_string(),
            openai_tts_url: "https://api.openai.com/v1/audio/speech".to_string(),
            r2_bucket_name: "uae-estate-vault".to_string(),
            r2_account_id: "mock_r2_account_id".to_string(),
            r2_access_key_id: "mock_r2_access_key".to_string(),
            r2_secret_access_key: "mock_r2_secret_key".to_string(),
            r2_endpoint_url: "https://mock_r2_account_id.r2.cloudflarestorage.com".to_string(),
            tone_model_api_key: "mock_tone_model_key".to_string(),
            upstash_vector_rest_url: env::var("UPSTASH_VECTOR_REST_URL")
                .or_else(|_| env::var("UPSTASH_VECTOR_URL"))
                .unwrap_or_default(),
            upstash_vector_rest_token: env::var("UPSTASH_VECTOR_REST_TOKEN")
                .or_else(|_| env::var("UPSTASH_VECTOR_TOKEN"))
                .unwrap_or_default(),
            node_env: "development".to_string(),
            admin_email: "admin@uae-estate.ae".to_string(),
            admin_password: "admin".to_string(),
            jwt_secret: env::var("JWT_SECRET")
                .unwrap_or_else(|_| "uae-real-estate-rbac-jwt-secret-key-2026-prod-grade".to_string()),
            neo4j_uri: env::var("NEO4J_URI").ok(),
            neo4j_username: env::var("NEO4J_USERNAME").ok(),
            neo4j_password: env::var("NEO4J_PASSWORD").ok(),
            neo4j_database: env::var("NEO4J_DATABASE").ok(),
            app_domain_name: env::var("APP_DOMAIN_NAME").unwrap_or_else(|_| "Real Estate".to_string()),
            app_currency: env::var("APP_CURRENCY").unwrap_or_else(|_| "AED".to_string()),
            kyc_document_name: env::var("KYC_DOCUMENT_NAME").unwrap_or_else(|_| "Emirates ID".to_string()),
            kyc_required_for_booking: true,
            kyc_unverified_message: "Viewing slot reservation requires verified {document_name} documents. Please upload your {document_name} in your user profile to unlock slot booking. In the meantime, I can assist you with recommendations, virtual tours, and price comparisons.".to_string(),
            domain_fallback_message: "I searched our portfolio, but couldn't find an exact match for that inquiry. Would you like to check our other featured listings or services?".to_string(),
            filter_primary_type_a: env::var("FILTER_PRIMARY_TYPE_A").unwrap_or_else(|_| "buy".to_string()),
            filter_primary_type_b: env::var("FILTER_PRIMARY_TYPE_B").unwrap_or_else(|_| "rent".to_string()),
            filter_type_a_synonyms: vec!["buy".to_string(), "sale".to_string(), "purchase".to_string(), "acquire".to_string()],
            filter_type_b_synonyms: vec!["rent".to_string(), "lease".to_string(), "leasing".to_string(), "annual".to_string()],
            filter_feature_1_synonyms: vec!["pet".to_string(), "dog".to_string(), "cat".to_string(), "animal".to_string()],
            filter_feature_2_synonyms: vec!["metro".to_string(), "commute".to_string(), "station".to_string(), "subway".to_string(), "train".to_string(), "transit".to_string()],
            action_booking_keywords: vec!["book".to_string(), "reserve".to_string(), "schedule".to_string(), "viewing".to_string(), "slot".to_string(), "appointment".to_string()],
            llm_api_key: "sk-proj-mock-key".to_string(),
            llm_endpoint_url: "https://api.openai.com/v1/chat/completions".to_string(),
            llm_model: "gpt-4o-mini".to_string(),
            agent_system_prompt: env::var("AGENT_SYSTEM_PROMPT").unwrap_or_default(),
            agent_context_prompt_template: env::var("AGENT_CONTEXT_PROMPT_TEMPLATE")
                .unwrap_or_default(),
            agent_edge_prompt_template: env::var("AGENT_EDGE_PROMPT_TEMPLATE")
                .unwrap_or_default(),
            stt_prompt: env::var("STT_PROMPT").unwrap_or_default(),
            vad_speech_probability_threshold: env::var("VAD_SPEECH_PROBABILITY_THRESHOLD")
                .ok().and_then(|value| value.parse().ok()).unwrap_or(0.15),
            vad_barge_in_probability_threshold: env::var("VAD_BARGE_IN_PROBABILITY_THRESHOLD")
                .ok().and_then(|value| value.parse().ok()).unwrap_or(0.40),
            vad_min_speech_amplitude: env::var("VAD_MIN_SPEECH_AMPLITUDE")
                .ok().and_then(|value| value.parse().ok()).unwrap_or(100),
            vad_energy_bypass_amplitude: env::var("VAD_ENERGY_BYPASS_AMPLITUDE")
                .ok().and_then(|value| value.parse().ok()).unwrap_or(600),
            vad_barge_in_amplitude: env::var("VAD_BARGE_IN_AMPLITUDE")
                .ok().and_then(|value| value.parse().ok()).unwrap_or(200),
            vad_end_of_speech_timeout_ms: env::var("VAD_END_OF_SPEECH_TIMEOUT_MS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(650),
            decision_llm_api_key: "sk-proj-mock-key".to_string(),
            decision_llm_endpoint_url: "https://api.openai.com/v1/chat/completions".to_string(),
            decision_llm_model: env::var("DECISION_LLM_MODEL")
                .or_else(|_| env::var("DECISION_MODEL"))
                .unwrap_or_default(),
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
            rag_search_success_message: env::var("RAG_SEARCH_SUCCESS_MESSAGE")
                .unwrap_or_default(),
            contract_not_found_message: env::var("CONTRACT_NOT_FOUND_MESSAGE")
                .unwrap_or_default(),
        }
    }
}
