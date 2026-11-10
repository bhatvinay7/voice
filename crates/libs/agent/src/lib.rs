pub mod cache;
pub mod models_checker;

use cache::AgentContextReasoner;
use db::DatabaseStore;
use futures_util::{FutureExt, StreamExt};
pub use models_checker::{ModelCheckStatus, ModelChecker, SystemModelsVerificationReport};
use orchestrator::{AgentCmd, AgentEvent, QueueEvent, RunId, TaskId};
use pubsub::{PubSubBus, SystemEvent};
use rag::RAGPipeline;
use serde_json::json;
use std::collections::VecDeque;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::RwLock;
use tokio::sync::{broadcast, mpsc};
use tokio_util::sync::CancellationToken;
use tokio_util::time::DelayQueue;

static SHARED_LLM_HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

/// One connection-pooled HTTP client per agent process. `reqwest::Client` is
/// cheap to clone and reuses keep-alive connections across all sessions.
pub fn shared_llm_http_client() -> &'static reqwest::Client {
    SHARED_LLM_HTTP_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .pool_max_idle_per_host(32)
            .tcp_keepalive(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    })
}

#[derive(Clone)]
struct GenerationCancellation {
    token: Arc<RwLock<CancellationToken>>,
}

impl GenerationCancellation {
    fn new() -> Self {
        Self {
            token: Arc::new(RwLock::new(CancellationToken::new())),
        }
    }

    fn begin(&self) {
        let mut token = self
            .token
            .write()
            .expect("generation cancellation lock poisoned");
        *token = CancellationToken::new();
    }

    fn cancel(&self) {
        let token = self
            .token
            .read()
            .expect("generation cancellation lock poisoned")
            .clone();
        token.cancel();
    }

    fn current(&self) -> CancellationToken {
        self.token
            .read()
            .expect("generation cancellation lock poisoned")
            .clone()
    }
}

pub struct LlmAgentActor {
    pub run_id: RunId,
    pub db: DatabaseStore,
    pub rag: Arc<RAGPipeline>,
    pub pubsub: Arc<PubSubBus>,
    pub orch_tx: mpsc::Sender<QueueEvent>,
    pub is_muted: bool,
    pub session_history: Vec<String>,
    pub cached_listings: Vec<db::PropertyListing>,
    pub active_listing_id: Option<String>,
    pub conversation_tx: Option<broadcast::Sender<orchestrator::ConversationEvent>>,
    pub last_redis_transcript: String,
    pub active_graph_query_id: Option<String>,
    pub active_playback_generation: u64,
    generation_cancellation: GenerationCancellation,
}

struct RedisConversationResult {
    transcript: Result<Option<String>, String>,
}

struct RawToolContextForMainAgent {
    session_id: String,
    question: Option<String>,
    graph_context: Option<String>,
    tool_result: serde_json::Value,
}

fn fallback_listing_answer(tool_result: &serde_json::Value) -> Option<String> {
    let listings = tool_result.get("listings")?.as_array()?;
    let summaries = listings
        .iter()
        .filter_map(|listing| {
            let title = listing.get("title")?.as_str()?.trim();
            let location = listing
                .get("location")
                .and_then(|value| value.as_str())
                .unwrap_or("Dubai");
            let price = listing
                .get("price_aed")
                .and_then(|value| value.as_f64())
                .map(|value| format!("AED {:.0}", value))
                .unwrap_or_else(|| "price not listed".to_string());
            Some(format!("{title} in {location} ({price})"))
        })
        .take(3)
        .collect::<Vec<_>>();
    if summaries.is_empty() {
        None
    } else {
        Some(format!(
            "I found these options: {}. I can narrow them down by price, location, or property type if you want.",
            summaries.join("; ")
        ))
    }
}

/// Bridges verified tool data from the evaluator into the normal main-agent
/// answer step. Keeping this as an MPSC boundary prevents evaluator output
/// from being mistaken for caller-facing speech and makes channel failures
/// explicit instead of silently falling back to partial data.
async fn bridge_raw_tool_context_to_main_agent(
    payload: RawToolContextForMainAgent,
) -> Option<RawToolContextForMainAgent> {
    let (sender, mut receiver) = mpsc::channel(1);
    let session_id = payload.session_id.clone();
    let send_result = AssertUnwindSafe(async {
        sender
            .send(payload)
            .await
            .map_err(|_| "main-agent context channel closed")
    })
    .catch_unwind()
    .await;
    match send_result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            tracing::error!(session = %session_id, %error, "raw tool context could not reach main agent");
            return None;
        }
        Err(_) => {
            tracing::error!("raw tool context bridge panicked while sending");
            return None;
        }
    }

    match AssertUnwindSafe(receiver.recv()).catch_unwind().await {
        Ok(Some(payload)) => Some(payload),
        Ok(None) => {
            tracing::error!(session = %session_id, "raw tool context bridge closed before main-agent evaluation");
            None
        }
        Err(_) => {
            tracing::error!(session = %session_id, "raw tool context bridge panicked while receiving");
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ModelFailure {
    Cancelled,
    MissingCredentials,
    HttpStatus(u16, String),
    Transport(String),
    InvalidJson(String),
    MissingContent,
    Timeout,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ModelOutcome {
    Success(String),
    Failure(ModelFailure),
}

fn effective_question_is_ordinal_first(question: &str) -> bool {
    let normalized = question.to_lowercase();
    [
        "first property",
        "first listing",
        "first one",
        "initial property",
        "earliest property",
        "the first one",
    ]
    .iter()
    .any(|phrase| normalized.contains(phrase))
}

fn question_needs_redis_history(question: &str) -> bool {
    let normalized = question.to_lowercase();
    effective_question_is_ordinal_first(question)
        || deterministic_turn_mode(question) == "reference"
        || [
            "first line",
            "earlier",
            "at the beginning",
            "in the beginning",
            "before",
            "previously",
            "you said",
            "you mentioned",
            "you suggested",
            "first thing",
            "number one",
            "option one",
        ]
        .iter()
        .any(|phrase| normalized.contains(phrase))
}

fn merge_redis_history_context(memory_context: Option<String>, transcript: String) -> String {
    let parsed_context = memory_context
        .as_deref()
        .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
        .filter(serde_json::Value::is_object);
    let memory_context_present = parsed_context.is_some();
    let mut context = parsed_context.unwrap_or_else(|| serde_json::json!({}));
    context["redis_conversation_fallback"] = serde_json::Value::String(transcript);
    context["context_recovery_source"] = serde_json::Value::String(
        if memory_context_present {
            "in_memory_graph+redis_history"
        } else {
            "redis_history"
        }
        .to_string(),
    );
    context.to_string()
}

fn with_interruption_replay_guidance(prompt: String, context: &str) -> String {
    let has_pending_interruption = serde_json::from_str::<serde_json::Value>(context)
        .ok()
        .and_then(|value| value["interrupted_response_guidance"]["replay_prompt_pending"].as_bool())
        .unwrap_or_else(|| context.contains("\"interrupted_response_guidance\""));
    if has_pending_interruption {
        format!(
            "{prompt}\n\nCaller-awareness instruction: the graph context includes an assistant response that was interrupted before the caller heard it completely, together with the linked tool data. Do not assume the caller heard that response. If their latest message is ambiguous or asks about the cut-off response, ask briefly whether they want the remaining answer repeated or clarified, using that saved context. Do not speak until the caller has finished their turn."
        )
    } else {
        prompt
    }
}

fn recovery_reference_node_ids(context: &serde_json::Value, question: &str) -> Vec<String> {
    let mut candidates = context["node_references"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    candidates.sort_by(|left, right| {
        let left_turn = left["turn_id"].as_u64().unwrap_or(0);
        let right_turn = right["turn_id"].as_u64().unwrap_or(0);
        let left_version = left["graph_version"].as_u64().unwrap_or(0);
        let right_version = right["graph_version"].as_u64().unwrap_or(0);
        right_turn
            .cmp(&left_turn)
            .then_with(|| right_version.cmp(&left_version))
            .then_with(|| {
                right["node_id"]
                    .as_str()
                    .unwrap_or_default()
                    .cmp(left["node_id"].as_str().unwrap_or_default())
            })
    });

    // The result/answer branches are the useful subject anchors. Prefer them
    // over user and intent navigation nodes when recovering missing LLM IDs.
    let interrupted = candidates
        .iter()
        .filter(|reference| reference["replay_prompt_pending"].as_bool() == Some(true))
        .cloned()
        .collect::<Vec<_>>();
    let mut ranked = candidates
        .iter()
        .filter(|reference| {
            reference["replay_prompt_pending"].as_bool() != Some(true)
                && matches!(
                    reference["kind"].as_str(),
                    Some("AssistantResponse" | "ToolContext")
                )
        })
        .cloned()
        .collect::<Vec<_>>();
    if ranked.is_empty() {
        ranked = candidates
            .iter()
            .filter(|reference| reference["replay_prompt_pending"].as_bool() != Some(true))
            .cloned()
            .collect();
    }

    if effective_question_is_ordinal_first(question) {
        ranked.reverse();
    }

    let ordered = interrupted.into_iter().chain(ranked).collect::<Vec<_>>();
    ordered
        .iter()
        .filter_map(|reference| reference["node_id"].as_str())
        .map(str::trim)
        .filter(|node_id| !node_id.is_empty())
        .take(6)
        .map(str::to_string)
        .collect()
}

fn has_prior_discussion_context(graph_context: Option<&str>) -> bool {
    let Some(context) =
        graph_context.and_then(|raw| serde_json::from_str::<serde_json::Value>(raw.trim()).ok())
    else {
        return false;
    };

    if context["latest_context_node"]
        .as_str()
        .is_some_and(|node_id| !node_id.trim().is_empty())
    {
        return true;
    }
    if context["redis_conversation_fallback"]
        .as_str()
        .is_some_and(|transcript| !transcript.trim().is_empty())
    {
        return true;
    }

    let references = context["node_references"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    references.iter().any(|reference| {
        matches!(
            reference["kind"].as_str(),
            Some("AssistantResponse" | "ToolContext" | "GraphEnrichment")
        )
    }) || references
        .iter()
        .filter(|reference| reference["kind"].as_str() == Some("UserText"))
        .count()
        > 1
}

fn effective_turn_mode(mode: &str, graph_context: Option<&str>) -> String {
    if !has_prior_discussion_context(graph_context)
        && matches!(mode, "reference" | "confirmation" | "dependency")
    {
        return "general".to_string();
    }
    mode.to_string()
}

fn deterministic_turn_mode(question: &str) -> &'static str {
    let lower = question.to_lowercase();
    let normalized = lower.split_whitespace().collect::<Vec<_>>().join(" ");
    let confirmation = [
        "yes",
        "yeah",
        "yep",
        "correct",
        "confirm",
        "go ahead",
        "that is right",
        "is true",
        "is it true",
        "is that true",
        "no",
        "nope",
        "not that one",
    ];
    if confirmation.iter().any(|phrase| {
        normalized == *phrase
            || normalized.ends_with(&format!(" {}", phrase))
            || normalized.starts_with(&format!("{} ", phrase))
    }) {
        return "confirmation";
    }

    let reference = [
        "last discussion",
        "last conversation",
        "earlier",
        "previous",
        "last time",
        "what did we say",
        "what did you say",
        "that property",
        "that listing",
        "this property",
        "this listing",
        "same one",
        "same property",
        "the one before",
        "the last one",
        "the first one",
        "first property",
        "first listing",
        "first one",
        "initial property",
        "from before",
        "related to the last",
        "remind me",
        "about it",
        "about this",
        "tell me more about",
        "what about the",
    ];
    if reference.iter().any(|phrase| normalized.contains(phrase)) {
        return "reference";
    }
    if normalized == "it"
        || normalized.ends_with(" it")
        || normalized.starts_with("it ")
        || normalized.contains(" its ")
        || normalized.contains(" the first ")
        || normalized.contains(" that one")
    {
        return "reference";
    }

    let dependency = [
        "still running",
        "still processing",
        "wait for",
        "pending",
        "tool result",
        "search status",
        "what happened to",
    ];
    if dependency.iter().any(|phrase| normalized.contains(phrase)) {
        return "dependency";
    }
    "general"
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TurnPlan {
    Clarify(String),
    AnswerFromContext {
        resolved_graph_context: Option<String>,
    },
    Tool {
        message: Option<String>,
        depends_on_query_ids: Vec<String>,
        /// Resolved request from graph context. This lets a confirmation such
        /// as "yes" execute the prior proposed search, never search "yes".
        tool_query: Option<String>,
    },
    WaitForRelatedTool {
        depends_on_query_id: String,
        message: Option<String>,
    },
}

/// One Redis polling connection per agent session. The actor never blocks on a
/// network read: it only sends requests and receives results through MPSC.
async fn run_agent_redis_poller(
    db: DatabaseStore,
    requests: &mut mpsc::Receiver<String>,
    results: mpsc::Sender<RedisConversationResult>,
) {
    let mut connection = None;
    while let Some(session_id) = requests.recv().await {
        if connection.is_none() {
            connection = db.open_redis_poll_connection().await.ok();
        }
        let transcript = match AssertUnwindSafe(async {
            match connection.as_mut() {
                Some(connection) => {
                    let live = db
                        .load_redis_conversation_on(&session_id, connection)
                        .await?;
                    if live
                        .as_deref()
                        .is_some_and(|transcript| !transcript.trim().is_empty())
                    {
                        Ok(live)
                    } else {
                        Ok(db
                            .load_redis_summary_checkpoint(&session_id)
                            .await?
                            .map(|checkpoint| checkpoint.summary))
                    }
                }
                None => Err("Agent Redis polling connection unavailable".to_string()),
            }
        })
        .catch_unwind()
        .await
        {
            Ok(result) => result,
            Err(_) => Err("Agent Redis polling worker panicked".to_string()),
        };
        if transcript.is_err() {
            connection = None;
        }
        if results
            .send(RedisConversationResult { transcript })
            .await
            .is_err()
        {
            break;
        }
    }
}

impl LlmAgentActor {
    pub fn new(
        run_id: RunId,
        db: DatabaseStore,
        pubsub: Arc<PubSubBus>,
        orch_tx: mpsc::Sender<QueueEvent>,
    ) -> Self {
        let rag = Arc::new(RAGPipeline::new(db.clone()));
        Self {
            run_id,
            db,
            rag,
            pubsub,
            orch_tx,
            is_muted: false,
            session_history: Vec::new(),
            cached_listings: Vec::new(),
            active_listing_id: None,
            conversation_tx: None,
            last_redis_transcript: String::new(),
            active_graph_query_id: None,
            active_playback_generation: 0,
            generation_cancellation: GenerationCancellation::new(),
        }
    }

    pub fn get_active_listing(&self) -> Option<&db::PropertyListing> {
        if let Some(ref id) = self.active_listing_id {
            if let Some(found) = self.cached_listings.iter().find(|l| &l.id == id) {
                return Some(found);
            }
        }
        self.cached_listings.first()
    }

    /// Evaluates whether the agent has enough context from cached data to answer the user question directly.
    pub fn answer_from_cached_context(
        &self,
        user_prompt: &str,
        contextual_follow_up_phrases: &[String],
        contextual_follow_up_clarification: &str,
    ) -> Option<String> {
        let listing = self.get_active_listing()?;
        AgentContextReasoner::answer_from_cached_context(
            listing,
            user_prompt,
            contextual_follow_up_phrases,
            contextual_follow_up_clarification,
        )
    }

    /// Generic pluggable LLM completion supporting any OpenAI-compatible endpoint configured in .env
    async fn call_pluggable_llm_outcome(
        &self,
        system_prompt: &str,
        user_prompt: &str,
    ) -> ModelOutcome {
        let cfg = config::AppConfig::load().unwrap_or_else(|_| config::AppConfig::load_default());
        if cfg.llm_api_key.is_empty() || cfg.llm_endpoint_url.is_empty() {
            return ModelOutcome::Failure(ModelFailure::MissingCredentials);
        }

        let effective_system_prompt = if cfg.llm_response_language.is_empty() {
            system_prompt.to_string()
        } else {
            format!(
                "{} Always respond in {} regardless of the language the user speaks in.",
                system_prompt, cfg.llm_response_language
            )
        };
        let payload = json!({
            "model": cfg.llm_model,
            "messages": [
                { "role": "system", "content": effective_system_prompt },
                { "role": "user", "content": user_prompt }
            ],
            "temperature": 0.2,
            "max_tokens": 512
        });

        let cancellation = self.generation_cancellation.current();
        let response = tokio::select! {
            _ = cancellation.cancelled() => return ModelOutcome::Failure(ModelFailure::Cancelled),
            response = shared_llm_http_client()
                .post(&cfg.llm_endpoint_url)
                .bearer_auth(&cfg.llm_api_key)
                .json(&payload)
                .timeout(std::time::Duration::from_secs(12))
                .send() => response,
        };
        let response = match response {
            Ok(response) => response,
            Err(error) if error.is_timeout() => {
                return ModelOutcome::Failure(ModelFailure::Timeout)
            }
            Err(_error) if cancellation.is_cancelled() => {
                return ModelOutcome::Failure(ModelFailure::Cancelled)
            }
            Err(error) => return ModelOutcome::Failure(ModelFailure::Transport(error.to_string())),
        };

        if cancellation.is_cancelled() {
            return ModelOutcome::Failure(ModelFailure::Cancelled);
        }
        let status = response.status();
        if !status.is_success() {
            return ModelOutcome::Failure(ModelFailure::HttpStatus(
                status.as_u16(),
                status
                    .canonical_reason()
                    .unwrap_or("HTTP error")
                    .to_string(),
            ));
        }

        let value = tokio::select! {
            _ = cancellation.cancelled() => return ModelOutcome::Failure(ModelFailure::Cancelled),
            value = response.json::<serde_json::Value>() => value,
        };
        let value = match value {
            Ok(value) => value,
            Err(_error) if cancellation.is_cancelled() => {
                return ModelOutcome::Failure(ModelFailure::Cancelled)
            }
            Err(error) => {
                return ModelOutcome::Failure(ModelFailure::InvalidJson(error.to_string()))
            }
        };

        match value
            .pointer("/choices/0/message/content")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            Some(content) => ModelOutcome::Success(content.to_string()),
            None => ModelOutcome::Failure(ModelFailure::MissingContent),
        }
    }

    pub async fn call_pluggable_llm(
        &self,
        system_prompt: &str,
        user_prompt: &str,
    ) -> Option<String> {
        match self
            .call_pluggable_llm_outcome(system_prompt, user_prompt)
            .await
        {
            ModelOutcome::Success(content) => Some(content),
            ModelOutcome::Failure(failure) => {
                tracing::warn!(run = %self.run_id.0, ?failure, "main model completion failed");
                None
            }
        }
    }

    /// Fast pluggable decision model completion supporting lightweight models for ultra-low latency
    async fn call_pluggable_decision_llm_outcome(
        &self,
        system_prompt: &str,
        user_prompt: &str,
    ) -> ModelOutcome {
        let cfg = config::AppConfig::load().unwrap_or_else(|_| config::AppConfig::load_default());
        let api_key = if !cfg.fast_model_api_key.is_empty() {
            &cfg.fast_model_api_key
        } else if !cfg.decision_llm_api_key.is_empty() {
            &cfg.decision_llm_api_key
        } else {
            &cfg.llm_api_key
        };
        let endpoint = if !cfg.fast_model_endpoint_url.is_empty() {
            &cfg.fast_model_endpoint_url
        } else if !cfg.decision_llm_endpoint_url.is_empty() {
            &cfg.decision_llm_endpoint_url
        } else {
            &cfg.llm_endpoint_url
        };
        let model = if !cfg.fast_model.is_empty() {
            &cfg.fast_model
        } else if !cfg.decision_llm_model.is_empty() {
            &cfg.decision_llm_model
        } else {
            &cfg.llm_model
        };

        if api_key.is_empty() || endpoint.is_empty() {
            return ModelOutcome::Failure(ModelFailure::MissingCredentials);
        }

        // Append a language constraint when configured.
        let effective_system_prompt = if cfg.llm_response_language.is_empty() {
            system_prompt.to_string()
        } else {
            format!(
                "{} Always respond in {} regardless of the language the user speaks in.",
                system_prompt, cfg.llm_response_language
            )
        };

        let payload = json!({
            "model": model,
            "messages": [
                { "role": "system", "content": effective_system_prompt },
                { "role": "user", "content": user_prompt }
            ],
            "temperature": 0.1,
            "max_tokens": 256
        });

        let cancellation = self.generation_cancellation.current();
        let response = tokio::select! {
            _ = cancellation.cancelled() => return ModelOutcome::Failure(ModelFailure::Cancelled),
            response = shared_llm_http_client()
                .post(endpoint)
                .bearer_auth(api_key)
                .json(&payload)
                .timeout(std::time::Duration::from_secs(8))
                .send() => response,
        };
        let response = match response {
            Ok(response) => response,
            Err(error) if error.is_timeout() => {
                return ModelOutcome::Failure(ModelFailure::Timeout)
            }
            Err(_error) if cancellation.is_cancelled() => {
                return ModelOutcome::Failure(ModelFailure::Cancelled)
            }
            Err(error) => return ModelOutcome::Failure(ModelFailure::Transport(error.to_string())),
        };
        if cancellation.is_cancelled() {
            return ModelOutcome::Failure(ModelFailure::Cancelled);
        }
        let status = response.status();
        if !status.is_success() {
            return ModelOutcome::Failure(ModelFailure::HttpStatus(
                status.as_u16(),
                status
                    .canonical_reason()
                    .unwrap_or("HTTP error")
                    .to_string(),
            ));
        }

        let value = tokio::select! {
            _ = cancellation.cancelled() => return ModelOutcome::Failure(ModelFailure::Cancelled),
            value = response.json::<serde_json::Value>() => value,
        };
        let value = match value {
            Ok(value) => value,
            Err(_error) if cancellation.is_cancelled() => {
                return ModelOutcome::Failure(ModelFailure::Cancelled)
            }
            Err(error) => {
                return ModelOutcome::Failure(ModelFailure::InvalidJson(error.to_string()))
            }
        };
        match value
            .pointer("/choices/0/message/content")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            Some(content) => ModelOutcome::Success(content.to_string()),
            None => ModelOutcome::Failure(ModelFailure::MissingContent),
        }
    }

    pub async fn call_pluggable_decision_llm(
        &self,
        system_prompt: &str,
        user_prompt: &str,
    ) -> Option<String> {
        match self
            .call_pluggable_decision_llm_outcome(system_prompt, user_prompt)
            .await
        {
            ModelOutcome::Success(content) => Some(content),
            ModelOutcome::Failure(failure) => {
                tracing::warn!(run = %self.run_id.0, ?failure, "decision model completion failed");
                None
            }
        }
    }

    pub async fn run_agent_loop(mut rx: mpsc::Receiver<AgentCmd>, mut actor: LlmAgentActor) {
        let mut redis_refreshes = DelayQueue::new();
        let (redis_request_tx, mut redis_request_rx) = mpsc::channel::<String>(8);
        let (redis_result_tx, mut redis_result_rx) = mpsc::channel::<RedisConversationResult>(8);
        let redis_db = actor.db.clone();
        tokio::spawn(async move {
            run_agent_redis_poller(redis_db, &mut redis_request_rx, redis_result_tx).await;
        });
        let mut queued_commands: VecDeque<AgentCmd> = VecDeque::new();
        loop {
            let cmd = if let Some(cmd) = queued_commands.pop_front() {
                Some(cmd)
            } else {
                tokio::select! {
                    Some(cmd) = rx.recv() => Some(cmd),
                    Some(_) = redis_refreshes.next(), if actor.conversation_tx.is_some() => {
                        let _ = redis_request_tx.try_send(actor.run_id.call_id().to_string());
                        redis_refreshes.insert((), std::time::Duration::from_secs(2));
                        continue;
                    }
                    Some(result) = redis_result_rx.recv() => {
                        actor.accept_redis_conversation(result);
                        continue;
                    }
                    else => break,
                }
            };

            let Some(cmd) = cmd else { break };
            if matches!(&cmd, AgentCmd::CancelGeneration { run_id } if run_id == &actor.run_id) {
                actor.cancel_generation();
                continue;
            }

            let session_deleted =
                matches!(&cmd, AgentCmd::Cancel { run_id } if run_id == &actor.run_id);
            let installs_fanout = matches!(&cmd, AgentCmd::SetConversationFanout { .. });
            if matches!(&cmd, AgentCmd::Cancel { run_id } if run_id != &actor.run_id) {
                continue;
            }

            let session_run_id = actor.run_id.clone();
            let generation_cancellation = actor.generation_cancellation.clone();
            let orch_tx = actor.orch_tx.clone();
            let mut pending_redis_results = Vec::new();
            let (command_finished, terminal_cancelled) = {
                let fut = AssertUnwindSafe(actor.handle_command(cmd)).catch_unwind();
                tokio::pin!(fut);
                let mut command_finished = false;
                let mut terminal_cancelled = false;

                let mut redis_result_closed = false;
                let mut command_rx_closed = false;
                loop {
                    tokio::select! {
                        result = &mut fut => {
                            command_finished = true;
                            if let Err(panic) = result {
                                let _ = orch_tx.send(QueueEvent::Agent(AgentEvent::Failed {
                                    run_id: session_run_id.clone(),
                                    error: format!("Agent command panicked: {panic:?}"),
                                })).await;
                                tracing::error!(run = %session_run_id.0, "Agent command future panicked; session actor continues");
                            }
                            break;
                        }
                        next_cmd = rx.recv(), if !command_rx_closed => {
                            match next_cmd {
                                Some(next_cmd) => {
                                    if matches!(&next_cmd, AgentCmd::CancelGeneration { run_id } if run_id == &session_run_id) {
                                        generation_cancellation.cancel();
                                    } else if matches!(&next_cmd, AgentCmd::Cancel { run_id } if run_id == &session_run_id) {
                                        generation_cancellation.cancel();
                                        terminal_cancelled = true;
                                        break;
                                    } else if matches!(&next_cmd, AgentCmd::Cancel { run_id } if run_id != &session_run_id) {
                                        // Ignore commands belonging to another session.
                                    } else {
                                        queued_commands.push_back(next_cmd);
                                    }
                                }
                                None => {
                                    command_rx_closed = true;
                                    tracing::warn!(run = %session_run_id.0, "Agent command channel closed; preserving active command until completion");
                                }
                            }
                        }
                        Some(result) = redis_result_rx.recv(), if !redis_result_closed => {
                            pending_redis_results.push(result);
                        }
                        else => {
                            redis_result_closed = true;
                        }
                    }
                }
                drop(fut);
                (command_finished, terminal_cancelled)
            };
            for result in pending_redis_results.drain(..) {
                actor.accept_redis_conversation(result);
            }

            if terminal_cancelled || session_deleted {
                generation_cancellation.cancel();
                actor.conversation_tx = None;
                redis_refreshes.clear();
                break;
            }
            if command_finished && installs_fanout {
                redis_refreshes.insert((), std::time::Duration::from_secs(2));
            }
        }
        println!("[LlmAgentActor {}] Shutdown.", actor.run_id.0);
    }

    fn begin_generation(&self) {
        self.generation_cancellation.begin();
    }

    fn cancel_generation(&self) {
        self.generation_cancellation.cancel();
    }

    fn accept_redis_conversation(&mut self, result: RedisConversationResult) {
        match result.transcript {
            Ok(Some(transcript)) if transcript != self.last_redis_transcript => {
                self.last_redis_transcript = transcript.clone();
                self.session_history
                    .push(format!("[REDIS_CONVERSATION]: {transcript}"));
            }
            Ok(_) => {}
            Err(error) => {
                tracing::debug!(%error, run = %self.run_id.0, "Agent Redis refresh deferred")
            }
        }
    }

    async fn request_graph_context(
        &self,
        query_id: &str,
        timeout_ms: u64,
        node_ids: &[String],
        question: Option<&str>,
    ) -> Option<String> {
        let (reply_to, mut response_rx) = mpsc::channel(1);
        let memory_context = if let Err(error) = self
            .orch_tx
            .send(QueueEvent::GraphContextRequested {
                session_id: self.run_id.call_id().to_string(),
                query_id: query_id.to_string(),
                node_ids: node_ids.to_vec(),
                reply_to,
            })
            .await
        {
            tracing::warn!(run = %self.run_id.0, %error, query_id, "in-memory graph request could not be delivered");
            None
        } else {
            let cancellation = self.generation_cancellation.current();
            match tokio::select! {
                _ = cancellation.cancelled() => return None,
                response = tokio::time::timeout(
                    std::time::Duration::from_millis(timeout_ms),
                    response_rx.recv(),
                ) => response,
            } {
                Ok(Some(response)) => response.context,
                Ok(None) => {
                    tracing::warn!(run = %self.run_id.0, query_id, "in-memory graph response channel closed");
                    None
                }
                Err(_) => {
                    tracing::warn!(run = %self.run_id.0, query_id, timeout_ms, "in-memory graph context lookup timed out");
                    None
                }
            }
        };
        let cancellation = self.generation_cancellation.current();
        let needs_history =
            memory_context.is_none() || question.is_some_and(question_needs_redis_history);
        if !needs_history {
            return memory_context;
        }
        let transcript = tokio::select! {
            _ = cancellation.cancelled() => return None,
            transcript = self.load_redis_history_fallback() => transcript,
        };
        let Some(transcript) = transcript.filter(|text| !text.trim().is_empty()) else {
            return memory_context;
        };
        tracing::info!(
            run = %self.run_id.0,
            query_id,
            historical_question = question.is_some_and(question_needs_redis_history),
            memory_context_present = memory_context.is_some(),
            transcript_chars = transcript.chars().count(),
            "using session Redis conversation history as graph-context fallback"
        );
        Some(merge_redis_history_context(memory_context, transcript))
    }

    async fn load_redis_history_fallback(&self) -> Option<String> {
        let session_id = self.run_id.call_id();
        match self.db.load_redis_conversation(session_id).await {
            Ok(Some(transcript)) if !transcript.trim().is_empty() => return Some(transcript),
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(run = %self.run_id.0, %error, "could not load live Redis conversation history");
            }
        }
        match self.db.load_redis_summary_checkpoint(session_id).await {
            Ok(Some(checkpoint)) if !checkpoint.summary.trim().is_empty() => {
                return Some(checkpoint.summary);
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(run = %self.run_id.0, %error, "could not load Redis conversation summary checkpoint");
            }
        }
        (!self.last_redis_transcript.trim().is_empty()).then(|| self.last_redis_transcript.clone())
    }

    /// Uses the fast model as a bounded turn gate. It receives only the
    /// connected graph component for this question, not the whole session.
    async fn plan_turn(
        &self,
        query_id: Option<&str>,
        question: &str,
        graph_context: Option<&str>,
    ) -> TurnPlan {
        let started_at = std::time::Instant::now();
        let cfg = config::AppConfig::load().unwrap_or_else(|_| config::AppConfig::load_default());
        if cfg.agent_turn_planner_prompt_template.trim().is_empty() {
            tracing::warn!(run = %self.run_id.0, "fast turn planner prompt is empty; using deterministic tool fallback");
            return TurnPlan::Tool {
                message: None,
                depends_on_query_ids: Vec::new(),
                tool_query: None,
            };
        }
        let render = |template: &str| {
            let prompt = template
                .replace("__SYSTEM_PROMPT__", &cfg.agent_system_prompt)
                .replace("__USER_QUESTION__", question)
                .replace("__GRAPH_CONTEXT__", graph_context.unwrap_or("{}"));
            format!(
                "{prompt}\n\nCONTEXT CONTRACT: GRAPH_CONTEXT.memory is an actor-built, bounded session-memory packet. Use memory.active_subject and memory.latest_verified_result before inferring from node titles. memory.recent_turns contains full user, assistant, and verified tool content in chronological order. Prefer a context answer when these fields directly answer the current question; request a tool only when verified memory lacks the needed fact."
            )
        };
        // Route with a tiny fast-model contract first, then use the narrow
        // event prompt. If routing is unavailable, general planning remains
        // deterministic and does not block a caller turn.
        let deterministic_mode = deterministic_turn_mode(question);
        let mode = if cfg.agent_turn_router_prompt_template.trim().is_empty() {
            if matches!(
                deterministic_mode,
                "reference" | "confirmation" | "dependency"
            ) && !has_prior_discussion_context(graph_context)
            {
                "general".to_string()
            } else {
                deterministic_mode.to_string()
            }
        } else {
            let model_mode = self
                .call_pluggable_decision_llm(
                    &render(&cfg.agent_turn_router_prompt_template),
                    question,
                )
                .await
                .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw.trim()).ok())
                .and_then(|value| {
                    value
                        .get("mode")
                        .and_then(|mode| mode.as_str())
                        .map(str::to_string)
                })
                .filter(|mode| {
                    matches!(
                        mode.as_str(),
                        "confirmation" | "reference" | "dependency" | "general"
                    )
                });

            match deterministic_mode {
                // Reference/confirmation/dependency routing is only valid
                // when this session actually contains prior connected
                // discussion.  Fresh calls must never classify an ordinal or
                // pronoun as an historical reference merely because it used
                // a reference-looking word.  This prevents stale graph data
                // from another call or call-graph state from being reused.
                "reference" | "confirmation" | "dependency"
                    if has_prior_discussion_context(graph_context) =>
                {
                    deterministic_mode.to_string()
                }
                "reference" | "confirmation" | "dependency" => model_mode
                    .filter(|mode| mode != "reference" && mode != "confirmation")
                    .unwrap_or_else(|| "general".to_string()),
                _ => model_mode.unwrap_or_else(|| "general".to_string()),
            }
        };
        let mode = effective_turn_mode(&mode, graph_context);
        tracing::info!(
            run = %self.run_id.0,
            question = %question,
            deterministic_mode,
            selected_mode = %mode,
            graph_context_present = graph_context.is_some(),
            graph_context_has_prior = has_prior_discussion_context(graph_context),
            "session fast-model routing decision"
        );
        let case_template = match mode.as_str() {
            "confirmation"
                if !cfg
                    .agent_confirmation_planner_prompt_template
                    .trim()
                    .is_empty() =>
            {
                &cfg.agent_confirmation_planner_prompt_template
            }
            "reference"
                if !cfg
                    .agent_reference_planner_prompt_template
                    .trim()
                    .is_empty() =>
            {
                &cfg.agent_reference_planner_prompt_template
            }
            "dependency"
                if !cfg
                    .agent_dependency_planner_prompt_template
                    .trim()
                    .is_empty() =>
            {
                &cfg.agent_dependency_planner_prompt_template
            }
            _ => &cfg.agent_turn_planner_prompt_template,
        };
        let prompt = render(case_template);
        let Some(raw_plan) = self.call_pluggable_decision_llm(&prompt, question).await else {
            tracing::warn!(run = %self.run_id.0, elapsed_ms = started_at.elapsed().as_millis(), "fast turn planner unavailable; using bounded fallback");
            return TurnPlan::Tool {
                message: None,
                depends_on_query_ids: Vec::new(),
                tool_query: Some(question.to_string()),
            };
        };
        let json = raw_plan
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();
        let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
            tracing::warn!(run = %self.run_id.0, elapsed_ms = started_at.elapsed().as_millis(), "fast turn planner returned invalid JSON; using bounded fallback");
            return TurnPlan::Tool {
                message: None,
                depends_on_query_ids: Vec::new(),
                tool_query: Some(question.to_string()),
            };
        };
        let plan = match value.get("action").and_then(|action| action.as_str()) {
            Some("clarify") => {
                if mode.as_str() == "general" && !has_prior_discussion_context(graph_context) {
                    TurnPlan::Tool {
                        message: None,
                        depends_on_query_ids: Vec::new(),
                        tool_query: Some(question.to_string()),
                    }
                } else {
                    TurnPlan::Clarify(
                        value
                            .get("message")
                            .and_then(|message| message.as_str())
                            .filter(|message| !message.trim().is_empty())
                            .unwrap_or("Could you please rephrase your property question?")
                            .to_string(),
                    )
                }
            }
            Some("context") => {
                let mut node_ids = value
                    .get("reference_node_ids")
                    .and_then(|ids| ids.as_array())
                    .map(|ids| {
                        ids.iter()
                            .filter_map(|id| id.as_str())
                            .map(str::trim)
                            .filter(|id| !id.is_empty())
                            .take(6)
                            .map(str::to_string)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();

                // Reference mode is intentionally recovery-safe: if the fast
                // model returns context without IDs, derive bounded candidates
                // only from the actor-owned connected-reference index. The
                // actor still validates these IDs before any body is exposed.
                if node_ids.is_empty() && mode.as_str() == "reference" {
                    if let Some(context) = graph_context
                        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
                    {
                        node_ids = recovery_reference_node_ids(&context, question);
                    }
                }
                let resolved_graph_context =
                    if let (Some(query_id), false) = (query_id, node_ids.is_empty()) {
                        let cfg = config::AppConfig::load()
                            .unwrap_or_else(|_| config::AppConfig::load_default());
                        self.request_graph_context(
                            query_id,
                            cfg.graph_context_request_timeout_ms,
                            &node_ids,
                            Some(question),
                        )
                        .await
                    } else {
                        graph_context.map(str::to_string)
                    };
                TurnPlan::AnswerFromContext {
                    resolved_graph_context,
                }
            }
            Some("tool") => TurnPlan::Tool {
                message: value
                    .get("message")
                    .and_then(|message| message.as_str())
                    .filter(|message| !message.trim().is_empty())
                    .map(str::to_string),
                depends_on_query_ids: value
                    .get("depends_on_query_ids")
                    .and_then(|ids| ids.as_array())
                    .map(|ids| {
                        ids.iter()
                            .filter_map(|id| id.as_str())
                            .filter(|id| !id.trim().is_empty())
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default(),
                tool_query: value
                    .get("tool_query")
                    .and_then(|query| query.as_str())
                    .filter(|query| !query.trim().is_empty())
                    .map(str::to_string),
            },
            Some("wait") => value
                .get("depends_on_query_id")
                .and_then(|id| id.as_str())
                .filter(|id| !id.trim().is_empty())
                .map(|depends_on_query_id| TurnPlan::WaitForRelatedTool {
                    depends_on_query_id: depends_on_query_id.to_string(),
                    message: value
                        .get("message")
                        .and_then(|message| message.as_str())
                        .map(str::to_string),
                })
                // Never wait without a concrete source permit. A malformed
                // planner response falls back to a fresh tool decision.
                .unwrap_or(TurnPlan::Tool {
                    message: None,
                    depends_on_query_ids: Vec::new(),
                    tool_query: None,
                }),
            _ if has_prior_discussion_context(graph_context)
                && matches!(mode.as_str(), "reference" | "confirmation") =>
            {
                TurnPlan::Clarify(
                    "Could you please clarify which earlier property or result you mean?"
                        .to_string(),
                )
            }
            _ => TurnPlan::Tool {
                message: None,
                depends_on_query_ids: Vec::new(),
                tool_query: None,
            },
        };
        let action = match &plan {
            TurnPlan::Clarify(_) => "clarify",
            TurnPlan::AnswerFromContext { .. } => "context",
            TurnPlan::Tool { .. } => "tool",
            TurnPlan::WaitForRelatedTool { .. } => "wait",
        };
        tracing::info!(
            run = %self.run_id.0,
            action,
            graph_context_present = graph_context.is_some(),
            elapsed_ms = started_at.elapsed().as_millis(),
            "fast turn planner decision"
        );
        plan
    }

    /// Reconciliation is detached from caller-facing latency. This worker
    /// receives only one connected query component and can only propose an
    /// event; the owning SessionActor validates and applies it.
    fn schedule_graph_enrichment(
        &self,
        query_id: Option<String>,
        question: Option<String>,
        graph_context: Option<String>,
        response: String,
    ) {
        let Some(query_id) = query_id else {
            return;
        };
        let Some(question) = question else {
            return;
        };
        let cfg = config::AppConfig::load().unwrap_or_else(|_| config::AppConfig::load_default());
        if cfg.agent_graph_enrichment_prompt_template.trim().is_empty() {
            return;
        }
        let base_graph_version = graph_context
            .as_deref()
            .and_then(|context| serde_json::from_str::<serde_json::Value>(context).ok())
            .and_then(|json| json.get("graph_version").and_then(|value| value.as_u64()))
            .unwrap_or(0);
        let prompt = cfg
            .agent_graph_enrichment_prompt_template
            .replace("__SYSTEM_PROMPT__", &cfg.agent_system_prompt)
            .replace("__USER_QUESTION__", &question)
            .replace(
                "__GRAPH_CONTEXT__",
                graph_context.as_deref().unwrap_or("{}"),
            )
            .replace("__ASSISTANT_RESPONSE__", &response);
        let worker = LlmAgentActor::new(
            self.run_id.clone(),
            self.db.clone(),
            self.pubsub.clone(),
            self.orch_tx.clone(),
        );
        let session_id = self.run_id.call_id().to_string();
        let queue_tx = self.orch_tx.clone();
        tokio::spawn(async move {
            let result = AssertUnwindSafe(async {
                tracing::info!(
                    session = %session_id,
                    %query_id,
                    base_graph_version,
                    "graph improver worker started"
                );
                let enrichment = worker
                    .call_pluggable_decision_llm(&prompt, &question)
                    .await
                    .filter(|text| !text.trim().is_empty())
                    .unwrap_or_else(|| {
                        format!("Question answered: {question}; response: {response}")
                    });
                tracing::info!(
                    session = %session_id,
                    %query_id,
                    base_graph_version,
                    enrichment = %enrichment,
                    "graph improver worker produced graph data"
                );
                queue_tx
                    .send(QueueEvent::GraphEnrichmentReady {
                        session_id,
                        query_id,
                        base_graph_version,
                        enrichment,
                    })
                    .await
                    .map_err(|_| "session graph queue closed")
            })
            .catch_unwind()
            .await;
            match result {
                Ok(Ok(())) => {}
                Ok(Err(error)) => tracing::debug!(%error, "graph enrichment discarded"),
                Err(_) => {
                    tracing::error!("graph improver worker panicked; session graph unchanged")
                }
            }
        });
    }

    async fn publish_assistant_conversation(
        &self,
        event_type: &str,
        payload_json: String,
        relevance: orchestrator::ConversationRelevance,
    ) {
        if self.generation_cancellation.current().is_cancelled() {
            tracing::info!(
                run = %self.run_id.0,
                event_type,
                "skipping assistant conversation persistence for interrupted generation"
            );
            return;
        }
        if let Some(tx) = &self.conversation_tx {
            let _ = tx.send(orchestrator::ConversationEvent {
                session_id: self.run_id.call_id().to_string(),
                trace_id: format!("trace-{}", chrono::Utc::now().timestamp_millis()),
                event_type: event_type.to_string(),
                payload_json,
                relevance,
            });
        } else {
            // Direct agents without a broadcast channel (tests / tool runners) write to Redis
            // only when the exchange is relevant.
            if relevance.is_relevant() {
                let _ = self
                    .db
                    .push_redis_stream(
                        "stream:conversation",
                        self.run_id.call_id(),
                        &format!("trace-{}", chrono::Utc::now().timestamp_millis()),
                        event_type,
                        &payload_json,
                    )
                    .await;
            }
        }
    }

    pub async fn handle_command(&mut self, cmd: AgentCmd) {
        match cmd {
            AgentCmd::SetConversationFanout {
                run_id,
                conversation_tx,
            } => {
                if run_id == self.run_id {
                    self.conversation_tx = Some(conversation_tx);
                }
            }
            AgentCmd::Start { run_id, input } => {
                if run_id != self.run_id {
                    return;
                }
                self.begin_generation();
                println!(
                    "[LlmAgentActor] Started run {} with input: {}",
                    run_id.0, input
                );
                self.session_history.push(input);
            }

            AgentCmd::SetGraphQuery {
                run_id,
                query_id,
                playback_generation,
            } => {
                if run_id == self.run_id {
                    self.begin_generation();
                    self.active_graph_query_id = Some(query_id);
                    self.active_playback_generation = playback_generation;
                }
            }

            AgentCmd::SetAiListenMute { muted, run_id } => {
                if run_id != self.run_id {
                    return;
                }
                self.is_muted = muted;
                if muted {
                    println!(
                        "[LlmAgentActor] AI_AGENT_LISTEN_MUTE active: Human interviewer speaking."
                    );
                } else {
                    println!("[LlmAgentActor] AI_AGENT_LISTEN_MUTE inactive: Resuming voice AI assistant.");
                }
            }

            AgentCmd::UserInput {
                run_id,
                user_id,
                text,
                clean_text,
                noise,
                role,
            } => {
                if run_id != self.run_id {
                    return;
                }
                if self.is_muted {
                    println!("[LlmAgentActor] Muted! Ignoring input from role {:?}", role);
                    return;
                }
                tracing::info!(run = %run_id.0, chars = text.len(), "agent received final user input");

                let trace_id = format!("trace-{}", chrono::Utc::now().timestamp_millis());

                // Session actors already broadcast user input to S3 and Redis. Directly
                // constructed agents (tests or tools) retain durable persistence here.
                if self.conversation_tx.is_none() {
                    let _ = self
                        .db
                        .push_redis_stream(
                            "stream:conversation",
                            run_id.call_id(),
                            &trace_id,
                            "USER_PROMPT",
                            &json!({ "text": text, "role": role }).to_string(),
                        )
                        .await;
                }

                // Push the raw transcript to history for context continuity;
                // use the clean_text for all LLM / RAG calls below.
                self.session_history.push(text.clone());

                // If noise was heavy/moderate and nothing intelligible came through,
                // respond with the unclear-audio message rather than querying the LLM.
                if let Some(ref np) = noise {
                    if np.level == orchestrator::audio_gate::NoiseLevel::Heavy
                        && np.speech_ratio < 0.15
                    {
                        let cfg = config::AppConfig::load()
                            .unwrap_or_else(|_| config::AppConfig::load_default());
                        let unclear_msg = cfg.unclear_audio_message.clone();
                        tracing::warn!(
                            "[LlmAgentActor] Heavy noise utterance — asking user to repeat"
                        );
                        let _ = self
                            .orch_tx
                            .send(QueueEvent::Agent(AgentEvent::Finished {
                                run_id: run_id.clone(),
                                query_id: self.active_graph_query_id.clone(),
                                playback_generation: self.active_playback_generation,
                                result: unclear_msg.clone(),
                                relevance: orchestrator::ConversationRelevance::Irrelevant,
                            }))
                            .await;
                        return;
                    }
                }

                let cfg =
                    config::AppConfig::load().unwrap_or_else(|_| config::AppConfig::load_default());
                // Use the cleaned text for booking detection and RAG.
                let effective_text = if clean_text.is_empty() {
                    &text
                } else {
                    &clean_text
                };
                let lower = effective_text.to_lowercase();
                let is_booking = cfg
                    .action_booking_keywords
                    .iter()
                    .any(|k| lower.contains(&k.to_lowercase()));

                // A verified active listing is the lowest-latency and most
                // reliable source for a property-detail follow-up.  Resolve
                // those fields before invoking the planner: a planner's
                // conservative `tool` fallback must not discard a listing
                // the caller and agent are already discussing.
                if !is_booking {
                    if let Some(cached_response) = self.answer_from_cached_context(
                        effective_text,
                        &cfg.contextual_follow_up_phrases,
                        &cfg.contextual_follow_up_clarification,
                    ) {
                        tracing::info!(
                            run = %run_id.0,
                            question = %effective_text,
                            "answered property follow-up from verified active-listing cache"
                        );
                        self.publish_assistant_conversation(
                            "LLM_RESPONSE",
                            serde_json::json!({
                                "response_text": cached_response,
                                "source": "session_cache"
                            })
                            .to_string(),
                            orchestrator::ConversationRelevance::Relevant,
                        )
                        .await;
                        let _ = self
                            .orch_tx
                            .send(QueueEvent::Agent(AgentEvent::Finished {
                                run_id: run_id.clone(),
                                query_id: self.active_graph_query_id.clone(),
                                playback_generation: self.active_playback_generation,
                                result: cached_response,
                                relevance: orchestrator::ConversationRelevance::Relevant,
                            }))
                            .await;
                        return;
                    }
                }
                let graph_context = if let Some(query_id) = self.active_graph_query_id.as_deref() {
                    self.request_graph_context(
                        query_id,
                        cfg.graph_context_request_timeout_ms,
                        &[],
                        Some(effective_text),
                    )
                    .await
                } else {
                    None
                };
                let turn_plan = self
                    .plan_turn(
                        self.active_graph_query_id.as_deref(),
                        effective_text,
                        graph_context.as_deref(),
                    )
                    .await;
                let planner_wait_message = match &turn_plan {
                    TurnPlan::Tool { message, .. } => message.clone(),
                    _ => None,
                };
                let planner_dependencies = match &turn_plan {
                    TurnPlan::Tool {
                        depends_on_query_ids,
                        ..
                    } => depends_on_query_ids.clone(),
                    _ => Vec::new(),
                };
                let planner_tool_query = match &turn_plan {
                    TurnPlan::Tool { tool_query, .. } => tool_query.clone(),
                    _ => None,
                };
                match turn_plan {
                    TurnPlan::Clarify(question) => {
                        tracing::info!(run = %run_id.0, "fast turn planner requested clarification before tool execution");
                        let _ = self
                            .orch_tx
                            .send(QueueEvent::Agent(AgentEvent::NeedsUserInput {
                                run_id,
                                question,
                            }))
                            .await;
                        return;
                    }
                    TurnPlan::WaitForRelatedTool {
                        depends_on_query_id,
                        message,
                    } => {
                        let Some(query_id) = self.active_graph_query_id.clone() else {
                            // A wait without immutable query identity cannot
                            // receive a safe answer permit.
                            return;
                        };
                        let _ = self
                            .orch_tx
                            .send(QueueEvent::Agent(AgentEvent::WaitForRelatedTool {
                                run_id,
                                query_id,
                                depends_on_query_id,
                                wait_message: message,
                            }))
                            .await;
                        return;
                    }
                    TurnPlan::AnswerFromContext {
                        resolved_graph_context,
                    } => {
                        // The fast model found enough connected context and,
                        // when it selected references, the exact node bodies
                        // were resolved by the owning SessionActor. The fast
                        // model remains a planner and never caller-facing.
                        if let Some(context) = resolved_graph_context.as_deref() {
                            let prompt = with_interruption_replay_guidance(
                                cfg.agent_context_prompt_template
                                    .replace("__SYSTEM_PROMPT__", &cfg.agent_system_prompt)
                                    .replace("__CONTEXT__", context)
                                    .replace("__DOMAIN__", "UAE real estate")
                                    .replace("__CURRENCY__", "AED")
                                    .replace(
                                        "__UNRELATED_QUERY_MESSAGE__",
                                        &cfg.unrelated_query_message,
                                    ),
                                context,
                            );
                            if let Some(answer) = self
                                .call_pluggable_llm(&prompt, effective_text)
                                .await
                                .filter(|answer| !answer.trim().is_empty())
                            {
                                let normalized_answer = answer.to_lowercase();
                                let needs_tool = normalized_answer.contains("[needs_tool]")
                                    || normalized_answer.contains("not available in the retrieved")
                                    || normalized_answer.contains("not found in the retrieved")
                                    || normalized_answer.contains("cannot find that information")
                                    || normalized_answer.contains("data is missing")
                                    || normalized_answer
                                        .contains("more current information is needed");
                                if !needs_tool {
                                    let clean_answer =
                                        answer.replace("[NEEDS_TOOL]", "").trim().to_string();
                                    if !clean_answer.is_empty() {
                                        self.publish_assistant_conversation(
                                            "LLM_RESPONSE",
                                            serde_json::json!({
                                                "response_text": clean_answer,
                                                "source": "graph_context_agent_model"
                                            })
                                            .to_string(),
                                            orchestrator::ConversationRelevance::Relevant,
                                        )
                                        .await;
                                        self.schedule_graph_enrichment(
                                            self.active_graph_query_id.clone(),
                                            Some(effective_text.to_string()),
                                            resolved_graph_context.clone(),
                                            clean_answer.clone(),
                                        );
                                        let _ = self
                                            .orch_tx
                                            .send(QueueEvent::Agent(AgentEvent::Finished {
                                                run_id: run_id.clone(),
                                                query_id: self.active_graph_query_id.clone(),
                                                playback_generation: self
                                                    .active_playback_generation,
                                                result: clean_answer,
                                                relevance:
                                                    orchestrator::ConversationRelevance::Relevant,
                                            }))
                                            .await;
                                        return;
                                    }
                                }
                                tracing::info!(
                                    run = %run_id.0,
                                    question = %effective_text,
                                    "main-agent context answer lacks requested data; escalating to property tool"
                                );
                            }
                        }
                        // The graph/DashMap result was insufficient. Fall through
                        // to the normal search tool path with the exact current
                        // user question instead of asking a redundant clarification.
                    }
                    TurnPlan::Tool { .. } => {}
                }
                // If deterministic cached-field handling did not match, give
                // the fast model the verified active property plus the
                // current graph component. This remains valid even when the
                // planner selected `tool`: that selection is a fallback, not
                // permission to forget the active listing.
                if !is_booking {
                    // If deterministic reasoner didn't match, check if we have active cached property
                    // and user is asking an edge question about it (not asking for a different location or explicit new search)
                    if let Some(listing) = self.get_active_listing() {
                        if !AgentContextReasoner::is_different_location_or_new_search(
                            effective_text,
                            listing,
                        ) {
                            let mut context_desc = format!(
                                "Active Property: {} in {} (ID: {}). Type: {}. Price: AED {}. Bedrooms: {}. Bathrooms: {}. Area: {} sqft. Pet Friendly: {}. Near Metro: {}. Description: {}. Available Slots: {:?}.",
                                listing.title, listing.location, listing.id, listing.listing_type, listing.price_aed, listing.bedrooms, listing.bathrooms, listing.area_sqft, listing.pet_friendly, listing.near_metro, listing.description, listing.available_slots
                            );
                            // Cached field matching did not answer this turn, so
                            // this is the narrow point where the decision LLM may
                            // need relationship context. Fetch only the current
                            // query's connected component, on demand.
                            if let Some(query_id) = self.active_graph_query_id.as_deref() {
                                if let Some(graph_context) = self
                                    .request_graph_context(
                                        query_id,
                                        cfg.graph_context_request_timeout_ms,
                                        &[],
                                        Some(effective_text),
                                    )
                                    .await
                                {
                                    context_desc.push_str("\nRelated conversation graph: ");
                                    context_desc.push_str(&graph_context);
                                }
                            }
                            let edge_prompt = with_interruption_replay_guidance(
                                cfg.agent_edge_prompt_template
                                    .replace("__SYSTEM_PROMPT__", &cfg.agent_system_prompt)
                                    .replace("__PROPERTY_CONTEXT__", &context_desc)
                                    .replace(
                                        "__UNRELATED_QUERY_MESSAGE__",
                                        &cfg.unrelated_query_message,
                                    ),
                                &context_desc,
                            );

                            let llm_reply = self
                                .call_pluggable_decision_llm(&edge_prompt, effective_text)
                                .await;

                            if let Some(edge_response) = llm_reply {
                                println!("[LlmAgentActor] Fast pluggable model answered edge question from active property context for run {}", run_id.0);

                                self.publish_assistant_conversation(
                                    "LLM_RESPONSE",
                                    serde_json::json!({
                                        "response_text": edge_response, "source": "llm_cached_context"
                                    }).to_string(),
                                    orchestrator::ConversationRelevance::Relevant,
                                ).await;

                                let _ = self
                                    .orch_tx
                                    .send(QueueEvent::Agent(AgentEvent::Finished {
                                        run_id: run_id.clone(),
                                        query_id: self.active_graph_query_id.clone(),
                                        playback_generation: self.active_playback_generation,
                                        result: edge_response,
                                        relevance: orchestrator::ConversationRelevance::Relevant,
                                    }))
                                    .await;
                                return;
                            }
                        }
                    }
                }

                let task_id = TaskId(format!(
                    "task-{}-{}",
                    run_id.0,
                    chrono::Utc::now().timestamp_millis()
                ));

                if is_booking {
                    let listing_id = self
                        .cached_listings
                        .iter()
                        .find(|l| {
                            lower.contains(&l.id.to_lowercase())
                                || lower.contains(&l.location.to_lowercase())
                        })
                        .map(|l| l.id.as_str())
                        .or_else(|| self.get_active_listing().map(|l| l.id.as_str()))
                        .unwrap_or_default();

                    if let Err(error) = self
                        .orch_tx
                        .send(QueueEvent::Agent(AgentEvent::ToolRequested {
                            run_id: run_id.clone(),
                            task_id,
                        query_id: self.active_graph_query_id.clone(),
                        playback_generation: self.active_playback_generation,
                        depends_on_query_ids: planner_dependencies.clone(),
                            tool_name: "book_viewing_slot".to_string(),
                            wait_message: planner_wait_message.clone(),
                            args: json!({
                                "listing_id": listing_id,
                                "user_id": user_id,
                                "slot_time": self.get_active_listing().and_then(|l| l.available_slots.first()).map(String::as_str).unwrap_or_default()
                            }),
                        }))
                        .await
                    {
                        tracing::error!(run = %run_id.0, %error, "booking tool request could not be dispatched");
                    } else {
                        tracing::info!(run = %run_id.0, tool = "book_viewing_slot", "agent dispatched tool request");
                    }
                } else {
                    if let Err(error) = self
                        .orch_tx
                        .send(QueueEvent::Agent(AgentEvent::ToolRequested {
                            run_id: run_id.clone(),
                            task_id,
                            query_id: self.active_graph_query_id.clone(),
                            playback_generation: self.active_playback_generation,
                            depends_on_query_ids: planner_dependencies,
                            tool_name: "search_properties".to_string(),
                            wait_message: planner_wait_message,
                            // Send the clean query to the RAG pipeline.
                            args: json!({ "query": planner_tool_query.as_deref().unwrap_or(effective_text) }),
                        }))
                        .await
                    {
                        tracing::error!(run = %run_id.0, %error, "property-search tool request could not be dispatched");
                    } else {
                        tracing::info!(run = %run_id.0, tool = "search_properties", "agent dispatched tool request");
                    }
                }
            }

            AgentCmd::ToolResult {
                run_id,
                task_id,
                query_id,
                outcome,
                user_question,
                graph_context,
            } => {
                if run_id != self.run_id {
                    return;
                }
                match outcome {
                    Ok(result_value) => {
                        // Tool execution is intentionally retained across
                        // barge-in, but the generation token used by the
                        // interrupted planner is not reusable. Start a
                        // fresh caller-facing generation before evaluator
                        // and main-agent work.
                        self.begin_generation();

                        // Cache listings returned from RAG property search
                        if let Some(listings_arr) =
                            result_value.get("listings").and_then(|v| v.as_array())
                        {
                            let mut new_listings = Vec::new();
                            for item in listings_arr {
                                if let Ok(listing) =
                                    serde_json::from_value::<db::PropertyListing>(item.clone())
                                {
                                    new_listings.push(listing);
                                }
                            }
                            if !new_listings.is_empty() {
                                self.active_listing_id = Some(new_listings[0].id.clone());
                                self.cached_listings = new_listings;
                            }
                        }

                        // Raw tool payloads, listing arrays, and response_text are
                        // context only. They must be rephrased against the exact
                        // user question by the main model, never spoken directly.
                        let base_speech_text = fallback_listing_answer(&result_value)
                            .unwrap_or_else(|| {
                                "I could not find a matching property listing yet. I can search again with any location, budget, or property type you prefer.".to_string()
                            });
                        let cfg = config::AppConfig::load()
                            .unwrap_or_else(|_| config::AppConfig::load_default());

                        // The evaluator is a separate main-agent step: it
                        // verifies coverage of the exact question before any
                        // caller-facing answer is emitted. It may request one
                        // refined search, while the original result remains
                        // attached to the session graph for follow-up use.
                        if !task_id.0.contains("-refine-")
                            && !cfg.agent_tool_evaluator_prompt_template.trim().is_empty()
                        {
                            let evaluator_prompt = cfg
                                .agent_tool_evaluator_prompt_template
                                .replace("__SYSTEM_PROMPT__", &cfg.agent_system_prompt)
                                .replace(
                                    "__USER_QUESTION__",
                                    user_question.as_deref().unwrap_or(""),
                                )
                                .replace("__TOOL_RESULT__", &result_value.to_string())
                                .replace(
                                    "__GRAPH_CONTEXT__",
                                    graph_context.as_deref().unwrap_or("{}"),
                                );
                            if let Some(raw) = self
                                .call_pluggable_decision_llm(
                                    &evaluator_prompt,
                                    user_question.as_deref().unwrap_or(""),
                                )
                                .await
                            {
                                let json = raw
                                    .trim()
                                    .trim_start_matches("```json")
                                    .trim_start_matches("```")
                                    .trim_end_matches("```")
                                    .trim();
                                if let Ok(evaluation) =
                                    serde_json::from_str::<serde_json::Value>(json)
                                {
                                    let decision = evaluation
                                        .get("decision")
                                        .and_then(|value| value.as_str())
                                        .unwrap_or("answer");
                                    if decision == "refine" {
                                        if let Some(refined_query) = evaluation
                                            .get("tool_query")
                                            .and_then(|value| value.as_str())
                                            .map(str::trim)
                                            .filter(|query| !query.is_empty())
                                        {
                                            let refined_task_id = TaskId(format!(
                                                "{}-refine-{}",
                                                task_id.0,
                                                chrono::Utc::now().timestamp_millis()
                                            ));
                                            tracing::info!(
                                                run = %run_id.0,
                                                task = %task_id.0,
                                                refined_task = %refined_task_id.0,
                                                query = %refined_query,
                                                reason = ?evaluation.get("reason"),
                                                "main-agent evaluator requested refined tool query"
                                            );
                                            let _ = self
                                                .orch_tx
                                                .send(QueueEvent::Agent(
                                                    AgentEvent::ToolRequested {
                                                        run_id,
                                                        task_id: refined_task_id,
                                                        query_id,
                                                        playback_generation: self
                                                            .active_playback_generation,
                                                        depends_on_query_ids: Vec::new(),
                                                        tool_name: "search_properties".to_string(),
                                                        wait_message: None,
                                                        args: json!({
                                                            "query": refined_query,
                                                            "query_already_refined": true
                                                        }),
                                                    },
                                                ))
                                                .await;
                                            return;
                                        }
                                    }
                                }
                            }
                        }

                        // A raw RAG result is context, not caller-facing
                        // speech. Generate an answer for the specific source
                        // question after the session actor has attached the
                        // result to that question's graph branch.
                        let main_agent_context =
                            bridge_raw_tool_context_to_main_agent(RawToolContextForMainAgent {
                                session_id: self.run_id.call_id().to_string(),
                                question: user_question.clone(),
                                graph_context: graph_context.clone(),
                                tool_result: result_value.clone(),
                            })
                            .await;
                        let speech_text = if let Some(main_agent_context) = main_agent_context {
                            let question = main_agent_context.question.as_deref();
                            let context = main_agent_context.graph_context.as_deref();
                            if let (Some(question), Some(context)) = (question, context) {
                                if cfg.agent_tool_result_prompt_template.trim().is_empty() {
                                    base_speech_text
                                } else {
                                    let prompt = with_interruption_replay_guidance(
                                        cfg.agent_tool_result_prompt_template
                                            .replace("__SYSTEM_PROMPT__", &cfg.agent_system_prompt)
                                            .replace("__USER_QUESTION__", question)
                                            .replace(
                                                "__TOOL_RESULT__",
                                                &main_agent_context.tool_result.to_string(),
                                            )
                                            .replace("__GRAPH_CONTEXT__", context),
                                        context,
                                    );
                                    self.call_pluggable_llm(&prompt, question)
                                        .await
                                        .filter(|answer| !answer.trim().is_empty())
                                        .unwrap_or_else(|| base_speech_text.clone())
                                }
                            } else {
                                base_speech_text
                            }
                        } else {
                            tracing::error!(
                                run = %run_id.0,
                                task = %task_id.0,
                                "main-agent received no raw tool context; suppressing fabricated answer"
                            );
                            return;
                        };

                        // executor.rs injects "domain_relevant": false when only the
                        // domain_fallback_message was returned (no property results).
                        let is_relevant = result_value
                            .get("domain_relevant")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(true);
                        let relevance = if is_relevant {
                            orchestrator::ConversationRelevance::Relevant
                        } else {
                            orchestrator::ConversationRelevance::Irrelevant
                        };

                        self.publish_assistant_conversation(
                            "LLM_RESPONSE",
                            serde_json::json!({
                                "response_text": speech_text, "task_id": task_id.0,
                                "domain_relevant": is_relevant,
                            })
                            .to_string(),
                            relevance,
                        )
                        .await;

                        // Tool-result evaluation is a main-agent response
                        // step. Do not send its prompt, raw result, or answer
                        // to the graph-improver/RAG optimizer. The tool result
                        // is already retained as a ToolContext node by the
                        // SessionActor and remains available to later turns.
                        tracing::info!(
                            run = %run_id.0,
                            task = %task_id.0,
                            query_id = ?query_id,
                            "main-agent evaluator result kept in graph; graph-improver input skipped"
                        );

                        let _ = self
                            .orch_tx
                            .send(QueueEvent::Agent(AgentEvent::Finished {
                                run_id: run_id.clone(),
                                query_id: query_id.clone(),
                                playback_generation: self.active_playback_generation,
                                result: speech_text,
                                relevance,
                            }))
                            .await;
                    }
                    Err(err) => {
                        let _ = self
                            .orch_tx
                            .send(QueueEvent::Agent(AgentEvent::Failed {
                                run_id: run_id.clone(),
                                error: format!("Tool {} failed: {}", task_id.0, err),
                            }))
                            .await;
                    }
                }
            }

            AgentCmd::CancelGeneration { run_id } => {
                if run_id == self.run_id {
                    self.cancel_generation();
                    tracing::debug!(run = %run_id.0, "session generation cancellation applied to active LLM requests");
                }
            }

            AgentCmd::ToolHeartbeat {
                run_id,
                task_id,
                progress_pct,
            } => {
                if run_id != self.run_id {
                    return;
                }
                println!(
                    "[LlmAgentActor] Run {}: Task {} heartbeat ({}%)",
                    run_id.0, task_id.0, progress_pct
                );
            }

            AgentCmd::ToolRetrying {
                run_id,
                task_id,
                attempt,
            } => {
                if run_id != self.run_id {
                    return;
                }
                tracing::info!(run = %run_id.0, task = %task_id.0, attempt, "tool retrying");
                let cfg =
                    config::AppConfig::load().unwrap_or_else(|_| config::AppConfig::load_default());
                if !cfg.tool_retry_tts_prompt.trim().is_empty() {
                    let _ = self
                        .orch_tx
                        .send(QueueEvent::Agent(AgentEvent::NeedsUserInput {
                            run_id,
                            question: cfg.tool_retry_tts_prompt,
                        }))
                        .await;
                }
            }

            AgentCmd::Cancel { run_id } => {
                if run_id != self.run_id {
                    return;
                }
                println!("[LlmAgentActor] Cancelled run {}", run_id.0);
                // Make deletion terminal even if the actor receives a command
                // raced with teardown. The loop observes this command and exits.
                self.conversation_tx = None;
                self.session_history.clear();
                self.cached_listings.clear();
                self.active_listing_id = None;
            }

            AgentCmd::ContinueLast { run_id, kind } => {
                use orchestrator::speech_filter::ContinuationKind;
                if run_id != self.run_id {
                    return;
                }
                if self.is_muted {
                    return;
                }

                let cfg =
                    config::AppConfig::load().unwrap_or_else(|_| config::AppConfig::load_default());

                // Derive a topic label from the active listing, falling back to the last history entry.
                let topic = if let Some(listing) = self.get_active_listing() {
                    format!("{} in {}", listing.title, listing.location)
                } else {
                    self.session_history
                        .iter()
                        .rev()
                        .find(|h| !h.is_empty())
                        .cloned()
                        .unwrap_or_else(|| "our previous topic".to_string())
                };

                // Find the last AI response in history (every other entry is AI).
                let last_ai_response: Option<String> = {
                    // session_history contains interleaved user + AI turns.
                    // Scan backwards for the last non-user-looking entry.
                    self.session_history
                        .iter()
                        .rev()
                        .skip(1) // skip the very last (which is the user's "repeat")
                        .find(|h| !h.starts_with('[') && h.len() > 20)
                        .cloned()
                };

                match kind {
                    ContinuationKind::Repeat => {
                        // Ask the LLM to rephrase the last answer concisely.
                        let system_prompt = cfg.agent_system_prompt.clone();
                        let user_prompt = format!(
                            "We are talking about: {}. The user asked you to repeat your last answer. \
                             Rephrase it concisely in 2-3 sentences, keeping all facts accurate. \
                             Previous answer: {}",
                            topic,
                            last_ai_response.as_deref().unwrap_or("(no previous answer available)")
                        );

                        tracing::info!(
                            "[LlmAgentActor] Handling Repeat continuation for run {}",
                            run_id.0
                        );
                        if let Some(reply) =
                            self.call_pluggable_llm(&system_prompt, &user_prompt).await
                        {
                            let prefixed = format!("We are talking about {}. {}", topic, reply);
                            self.publish_assistant_conversation(
                                "LLM_RESPONSE",
                                serde_json::json!({ "response_text": prefixed, "source": "continuation_repeat" }).to_string(),
                                orchestrator::ConversationRelevance::Relevant,
                            ).await;
                            let _ = self
                                .orch_tx
                                .send(QueueEvent::Agent(AgentEvent::Finished {
                                    run_id: run_id.clone(),
                                    query_id: self.active_graph_query_id.clone(),
                                    playback_generation: self.active_playback_generation,
                                    result: prefixed,
                                    relevance: orchestrator::ConversationRelevance::Relevant,
                                }))
                                .await;
                        }
                    }
                    ContinuationKind::Continue => {
                        // Continue from cached context without a new RAG search.
                        let system_prompt = cfg.agent_system_prompt.clone();
                        let user_prompt = format!(
                            "We are talking about: {}. The user wants you to continue from where you left off. \
                             Continue your explanation or description naturally, adding more relevant detail. \
                             Previous context: {}",
                            topic,
                            last_ai_response.as_deref().unwrap_or("(no previous context available)")
                        );

                        tracing::info!(
                            "[LlmAgentActor] Handling Continue continuation for run {}",
                            run_id.0
                        );
                        if let Some(reply) =
                            self.call_pluggable_llm(&system_prompt, &user_prompt).await
                        {
                            let prefixed = format!("We are talking about {}. {}", topic, reply);
                            self.publish_assistant_conversation(
                                "LLM_RESPONSE",
                                serde_json::json!({ "response_text": prefixed, "source": "continuation_continue" }).to_string(),
                                orchestrator::ConversationRelevance::Relevant,
                            ).await;
                            let _ = self
                                .orch_tx
                                .send(QueueEvent::Agent(AgentEvent::Finished {
                                    run_id: run_id.clone(),
                                    query_id: self.active_graph_query_id.clone(),
                                    playback_generation: self.active_playback_generation,
                                    result: prefixed,
                                    relevance: orchestrator::ConversationRelevance::Relevant,
                                }))
                                .await;
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/agent_agent_lib_tests_1.rs"]
mod tests;
