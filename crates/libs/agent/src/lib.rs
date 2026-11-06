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
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::sync::OnceLock;
use tokio::sync::{broadcast, mpsc};
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
}

struct RedisConversationResult {
    transcript: Result<Option<String>, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TurnPlan {
    Clarify(String),
    AnswerFromContext,
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
                    if live.is_some() {
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
    pub async fn call_pluggable_llm(
        &self,
        system_prompt: &str,
        user_prompt: &str,
    ) -> Option<String> {
        let cfg = config::AppConfig::load().unwrap_or_else(|_| config::AppConfig::load_default());
        if cfg.llm_api_key.is_empty() || cfg.llm_endpoint_url.is_empty() {
            return None;
        }

        // Append a language constraint to the system prompt when configured.
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

        let res = shared_llm_http_client()
            .post(&cfg.llm_endpoint_url)
            .bearer_auth(&cfg.llm_api_key)
            .json(&payload)
            .timeout(std::time::Duration::from_secs(12))
            .send()
            .await
            .ok()?;

        if res.status().is_success() {
            let resp_json: serde_json::Value = res.json().await.ok()?;
            let content = resp_json
                .pointer("/choices/0/message/content")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string());
            content
        } else {
            None
        }
    }

    /// Fast pluggable decision model completion supporting lightweight models for ultra-low latency
    pub async fn call_pluggable_decision_llm(
        &self,
        system_prompt: &str,
        user_prompt: &str,
    ) -> Option<String> {
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
            return None;
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

        let res = shared_llm_http_client()
            .post(endpoint)
            .bearer_auth(api_key)
            .json(&payload)
            .timeout(std::time::Duration::from_secs(8))
            .send()
            .await
            .ok()?;

        if res.status().is_success() {
            let resp_json: serde_json::Value = res.json().await.ok()?;
            resp_json
                .pointer("/choices/0/message/content")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
        } else {
            None
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
        loop {
            tokio::select! {
                Some(cmd) = rx.recv() => {
                    let session_deleted = matches!(&cmd, AgentCmd::Cancel { run_id } if run_id == &actor.run_id);
                    let installs_fanout = matches!(cmd, AgentCmd::SetConversationFanout { .. });
                    let fut = AssertUnwindSafe(actor.handle_command(cmd));
                    if fut.catch_unwind().await.is_err() {
                        eprintln!("[LlmAgentActor {}] Command panicked; actor continues", actor.run_id.0);
                    } else if session_deleted {
                        // Cancellation is terminal for this session. Dropping the
                        // request sender below closes the dedicated Redis poller;
                        // clearing the fanout prevents any late publication.
                        actor.conversation_tx = None;
                        redis_refreshes.clear();
                        break;
                    } else if installs_fanout {
                        redis_refreshes.insert((), std::time::Duration::from_secs(2));
                    }
                }
                Some(_) = redis_refreshes.next(), if actor.conversation_tx.is_some() => {
                    // A full queue means a refresh is already pending, so coalesce it.
                    let _ = redis_request_tx.try_send(actor.run_id.call_id().to_string());
                    redis_refreshes.insert((), std::time::Duration::from_secs(2));
                }
                Some(result) = redis_result_rx.recv() => actor.accept_redis_conversation(result),
                else => break,
            }
        }
        println!("[LlmAgentActor {}] Shutdown.", actor.run_id.0);
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

    async fn request_graph_context(&self, query_id: &str, timeout_ms: u64) -> Option<String> {
        let (reply_to, mut response_rx) = mpsc::channel(1);
        self.orch_tx
            .send(QueueEvent::GraphContextRequested {
                session_id: self.run_id.call_id().to_string(),
                query_id: query_id.to_string(),
                reply_to,
            })
            .await
            .ok()?;
        let response = tokio::time::timeout(
            std::time::Duration::from_millis(timeout_ms),
            response_rx.recv(),
        )
        .await
        .ok()??;
        response.context
    }

    /// Uses the fast model as a bounded turn gate. It receives only the
    /// connected graph component for this question, not the whole session.
    async fn plan_turn(&self, question: &str, graph_context: Option<&str>) -> TurnPlan {
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
            template
                .replace("__SYSTEM_PROMPT__", &cfg.agent_system_prompt)
                .replace("__USER_QUESTION__", question)
                .replace("__GRAPH_CONTEXT__", graph_context.unwrap_or("{}"))
        };
        // Route with a tiny fast-model contract first, then use the narrow
        // event prompt. If routing is unavailable, general planning remains
        // deterministic and does not block a caller turn.
        let mode = if cfg.agent_turn_router_prompt_template.trim().is_empty() {
            "general".to_string()
        } else {
            self.call_pluggable_decision_llm(
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
            })
            .unwrap_or_else(|| "general".to_string())
        };
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
            tracing::warn!(run = %self.run_id.0, elapsed_ms = started_at.elapsed().as_millis(), "fast turn planner unavailable; using deterministic tool fallback");
            return TurnPlan::Tool {
                message: None,
                depends_on_query_ids: Vec::new(),
                tool_query: None,
            };
        };
        let json = raw_plan
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();
        let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
            tracing::warn!(run = %self.run_id.0, elapsed_ms = started_at.elapsed().as_millis(), "fast turn planner returned invalid JSON; using deterministic tool fallback");
            return TurnPlan::Tool {
                message: None,
                depends_on_query_ids: Vec::new(),
                tool_query: None,
            };
        };
        let plan = match value.get("action").and_then(|action| action.as_str()) {
            Some("clarify") => TurnPlan::Clarify(
                value
                    .get("message")
                    .and_then(|message| message.as_str())
                    .filter(|message| !message.trim().is_empty())
                    .unwrap_or("Could you please rephrase your property question?")
                    .to_string(),
            ),
            Some("context") => TurnPlan::AnswerFromContext,
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
            _ => TurnPlan::Tool {
                message: None,
                depends_on_query_ids: Vec::new(),
                tool_query: None,
            },
        };
        let action = match &plan {
            TurnPlan::Clarify(_) => "clarify",
            TurnPlan::AnswerFromContext => "context",
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
            // A model outage must not affect the already-delivered answer.
            // Use a bounded deterministic audit annotation as fallback.
            let enrichment = worker
                .call_pluggable_decision_llm(&prompt, &question)
                .await
                .filter(|text| !text.trim().is_empty())
                .unwrap_or_else(|| format!("Question answered: {question}; response: {response}"));
            if queue_tx
                .send(QueueEvent::GraphEnrichmentReady {
                    session_id,
                    query_id,
                    base_graph_version,
                    enrichment,
                })
                .await
                .is_err()
            {
                tracing::debug!("graph enrichment discarded because session queue closed");
            }
        });
    }

    async fn publish_assistant_conversation(
        &self,
        event_type: &str,
        payload_json: String,
        relevance: orchestrator::ConversationRelevance,
    ) {
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

            AgentCmd::InjectToneContext {
                run_id,
                session_id,
                tone_intent,
                confidence,
            } => {
                if run_id != self.run_id {
                    return;
                }
                println!("[LlmAgentActor] Run {} (Session {}): Received Tone Intent: {} (Confidence: {})", run_id.0, session_id, tone_intent, confidence);
                self.session_history.push(format!(
                    "[SYSTEM_TONE_INJECTION]: Caller tone detected as {} ({})",
                    tone_intent, confidence
                ));
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
                        let _ = self.pubsub.publish(pubsub::SystemEvent::VoiceTtsStream {
                            call_id: run_id.call_id().to_string(),
                            audio_chunk_base64: format!("audio_chunk_{}", unclear_msg.len()),
                            index: 0,
                            is_end: true,
                        });
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
                let graph_context = if let Some(query_id) = self.active_graph_query_id.as_deref() {
                    self.request_graph_context(query_id, cfg.graph_context_request_timeout_ms)
                        .await
                } else {
                    None
                };
                let turn_plan = self
                    .plan_turn(effective_text, graph_context.as_deref())
                    .await;
                let planner_requires_tool = matches!(&turn_plan, TurnPlan::Tool { .. });
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
                    TurnPlan::AnswerFromContext => {
                        // The fast model found enough connected context. The
                        // response still belongs to the main agent model: the
                        // fast model is a planner, never caller-facing.
                        if let Some(context) = graph_context.as_deref() {
                            let prompt = cfg
                                .agent_context_prompt_template
                                .replace("__SYSTEM_PROMPT__", &cfg.agent_system_prompt)
                                .replace("__CONTEXT__", context)
                                .replace("__DOMAIN__", "UAE real estate")
                                .replace("__CURRENCY__", "AED")
                                .replace(
                                    "__UNRELATED_QUERY_MESSAGE__",
                                    &cfg.unrelated_query_message,
                                );
                            if let Some(answer) = self
                                .call_pluggable_llm(&prompt, effective_text)
                                .await
                                .filter(|answer| !answer.trim().is_empty())
                            {
                                self.publish_assistant_conversation(
                                    "LLM_RESPONSE",
                                    serde_json::json!({
                                        "response_text": answer,
                                        "source": "graph_context_agent_model"
                                    })
                                    .to_string(),
                                    orchestrator::ConversationRelevance::Relevant,
                                )
                                .await;
                                self.schedule_graph_enrichment(
                                    self.active_graph_query_id.clone(),
                                    Some(effective_text.to_string()),
                                    graph_context.clone(),
                                    answer.clone(),
                                );
                                let _ = self
                                    .orch_tx
                                    .send(QueueEvent::Agent(AgentEvent::Finished {
                                        run_id: run_id.clone(),
                                        query_id: self.active_graph_query_id.clone(),
                                        playback_generation: self.active_playback_generation,
                                        result: answer,
                                        relevance: orchestrator::ConversationRelevance::Relevant,
                                    }))
                                    .await;
                                return;
                            }
                        }
                        // A malformed/empty fast answer must not turn into a
                        // blind tool call; ask the caller to make the subject
                        // explicit instead.
                        let _ = self
                            .orch_tx
                            .send(QueueEvent::Agent(AgentEvent::NeedsUserInput {
                                run_id,
                                question: "Could you please rephrase the property detail you need?"
                                    .to_string(),
                            }))
                            .await;
                        return;
                    }
                    TurnPlan::Tool { .. } => {}
                }
                let lower = effective_text.to_lowercase();
                let is_booking = cfg
                    .action_booking_keywords
                    .iter()
                    .any(|k| lower.contains(&k.to_lowercase()));

                // Evaluate whether we already have sufficient context from previous searches to answer directly
                // A fast-model `tool` decision is authoritative. It avoids a
                // second, heuristic response path that could bypass the agent
                // model or use stale cached listing data.
                if !is_booking && !planner_requires_tool {
                    if let Some(cached_response) = self.answer_from_cached_context(
                        effective_text,
                        &cfg.contextual_follow_up_phrases,
                        &cfg.contextual_follow_up_clarification,
                    ) {
                        println!("[LlmAgentActor] Answering follow-up question directly from cached property context for run {}", run_id.0);

                        self.publish_assistant_conversation(
                            "LLM_RESPONSE",
                            serde_json::json!({
                                "response_text": cached_response, "source": "session_cache"
                            })
                            .to_string(),
                            orchestrator::ConversationRelevance::Relevant,
                        )
                        .await;

                        let _ = self.pubsub.publish(pubsub::SystemEvent::VoiceTtsStream {
                            call_id: run_id.call_id().to_string(),
                            audio_chunk_base64: format!("audio_chunk_{}", cached_response.len()),
                            index: 0,
                            is_end: true,
                        });

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
                                    )
                                    .await
                                {
                                    context_desc.push_str("\nRelated conversation graph: ");
                                    context_desc.push_str(&graph_context);
                                }
                            }
                            let edge_prompt = cfg
                                .agent_edge_prompt_template
                                .replace("__SYSTEM_PROMPT__", &cfg.agent_system_prompt)
                                .replace("__PROPERTY_CONTEXT__", &context_desc)
                                .replace(
                                    "__UNRELATED_QUERY_MESSAGE__",
                                    &cfg.unrelated_query_message,
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

                                let _ = self.pubsub.publish(pubsub::SystemEvent::VoiceTtsStream {
                                    call_id: run_id.call_id().to_string(),
                                    audio_chunk_base64: format!(
                                        "audio_chunk_{}",
                                        edge_response.len()
                                    ),
                                    index: 0,
                                    is_end: true,
                                });

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

                        let base_speech_text = result_value
                            .get("response_text")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Action completed successfully.")
                            .to_string();

                        // A raw RAG result is context, not caller-facing
                        // speech. Generate an answer for the specific source
                        // question after the session actor has attached the
                        // result to that question's graph branch.
                        let speech_text = if let (Some(question), Some(context)) =
                            (user_question.as_deref(), graph_context.as_deref())
                        {
                            let cfg = config::AppConfig::load()
                                .unwrap_or_else(|_| config::AppConfig::load_default());
                            if cfg.agent_tool_result_prompt_template.trim().is_empty() {
                                base_speech_text
                            } else {
                                let prompt = cfg
                                    .agent_tool_result_prompt_template
                                    .replace("__SYSTEM_PROMPT__", &cfg.agent_system_prompt)
                                    .replace("__USER_QUESTION__", question)
                                    .replace("__TOOL_RESULT__", &result_value.to_string())
                                    .replace("__GRAPH_CONTEXT__", context);
                                self.call_pluggable_llm(&prompt, question)
                                    .await
                                    .filter(|answer| !answer.trim().is_empty())
                                    .unwrap_or(base_speech_text)
                            }
                        } else {
                            base_speech_text
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

                        let _ = self.pubsub.publish(SystemEvent::VoiceTtsStream {
                            call_id: run_id.call_id().to_string(),
                            audio_chunk_base64: format!("audio_chunk_{}", speech_text.len()),
                            index: 0,
                            is_end: true,
                        });

                        self.schedule_graph_enrichment(
                            query_id.clone(),
                            user_question.clone(),
                            graph_context.clone(),
                            speech_text.clone(),
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
                    tracing::debug!(run = %run_id.0, "generation cancellation acknowledged; no graph model task is active");
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
                            let _ = self.pubsub.publish(pubsub::SystemEvent::VoiceTtsStream {
                                call_id: run_id.call_id().to_string(),
                                audio_chunk_base64: format!("audio_chunk_{}", prefixed.len()),
                                index: 0,
                                is_end: true,
                            });
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
                            let _ = self.pubsub.publish(pubsub::SystemEvent::VoiceTtsStream {
                                call_id: run_id.call_id().to_string(),
                                audio_chunk_base64: format!("audio_chunk_{}", prefixed.len()),
                                index: 0,
                                is_end: true,
                            });
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
