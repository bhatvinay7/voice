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
use tokio::sync::{broadcast, mpsc};
use tokio_util::time::DelayQueue;

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
}

struct RedisConversationResult {
    transcript: Result<Option<String>, String>,
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
                Some(connection) => db.load_redis_conversation_on(&session_id, connection).await,
                None => Err("Agent Redis polling connection unavailable".to_string()),
            }
        }).catch_unwind().await {
            Ok(result) => result,
            Err(_) => Err("Agent Redis polling worker panicked".to_string()),
        };
        if transcript.is_err() {
            connection = None;
        }
        if results.send(RedisConversationResult { transcript }).await.is_err() {
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
    pub fn answer_from_cached_context(&self, user_prompt: &str) -> Option<String> {
        let listing = self.get_active_listing()?;
        AgentContextReasoner::answer_from_cached_context(listing, user_prompt)
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

        let payload = json!({
            "model": cfg.llm_model,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": user_prompt }
            ],
            "temperature": 0.2,
            "max_tokens": 512
        });

        let client = reqwest::Client::new();
        let res = client
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

        let payload = json!({
            "model": model,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": user_prompt }
            ],
            "temperature": 0.1,
            "max_tokens": 256
        });

        let client = reqwest::Client::new();
        let res = client
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
                    let installs_fanout = matches!(cmd, AgentCmd::SetConversationFanout { .. });
                    let fut = AssertUnwindSafe(actor.handle_command(cmd));
                    if fut.catch_unwind().await.is_err() {
                        eprintln!("[LlmAgentActor {}] Command panicked; actor continues", actor.run_id.0);
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
                self.session_history.push(format!("[REDIS_CONVERSATION]: {transcript}"));
            }
            Ok(_) => {}
            Err(error) => tracing::debug!(%error, run = %self.run_id.0, "Agent Redis refresh deferred"),
        }
    }

    async fn publish_assistant_conversation(&self, event_type: &str, payload_json: String) {
        if let Some(tx) = &self.conversation_tx {
            let _ = tx.send(orchestrator::ConversationEvent {
                session_id: self.run_id.call_id().to_string(),
                trace_id: format!("trace-{}", chrono::Utc::now().timestamp_millis()),
                event_type: event_type.to_string(),
                payload_json,
            });
        } else {
            let _ = self.db.push_redis_stream(
                "stream:conversation", self.run_id.call_id(),
                &format!("trace-{}", chrono::Utc::now().timestamp_millis()), event_type, &payload_json,
            ).await;
        }
    }

    pub async fn handle_command(&mut self, cmd: AgentCmd) {
        match cmd {
            AgentCmd::SetConversationFanout { run_id, conversation_tx } => {
                if run_id == self.run_id { self.conversation_tx = Some(conversation_tx); }
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
                run_id, user_id, text, role
            } => {
                if run_id != self.run_id {
                    return;
                }
                if self.is_muted {
                    println!("[LlmAgentActor] Muted! Ignoring input from role {:?}", role);
                    return;
                }

                let trace_id = format!("trace-{}", chrono::Utc::now().timestamp_millis());

                // Session actors already broadcast user input to S3 and Redis. Directly
                // constructed agents (tests or tools) retain durable persistence here.
                if self.conversation_tx.is_none() {
                    let _ = self.db.push_redis_stream(
                        "stream:conversation", run_id.call_id(), &trace_id, "USER_PROMPT",
                        &json!({ "text": text, "role": role }).to_string(),
                    ).await;
                }

                self.session_history.push(text.clone());

                let cfg =
                    config::AppConfig::load().unwrap_or_else(|_| config::AppConfig::load_default());
                let lower = text.to_lowercase();
                let is_booking = cfg
                    .action_booking_keywords
                    .iter()
                    .any(|k| lower.contains(&k.to_lowercase()));

                // Evaluate whether we already have sufficient context from previous searches to answer directly
                if !is_booking {
                    if let Some(cached_response) = self.answer_from_cached_context(&text) {
                        println!("[LlmAgentActor] Answering follow-up question directly from cached property context for run {}", run_id.0);

                        self.publish_assistant_conversation("LLM_RESPONSE", json!({
                            "response_text": cached_response, "source": "session_cache"
                        }).to_string()).await;

                        let _ = self.pubsub.publish(SystemEvent::VoiceTtsStream {
                            call_id: run_id.call_id().to_string(),
                            audio_chunk_base64: format!("audio_chunk_{}", cached_response.len()),
                            index: 0,
                            is_end: true,
                        });

                        let _ = self
                            .orch_tx
                            .send(QueueEvent::Agent(AgentEvent::Finished {
                                run_id: run_id.clone(),
                                result: cached_response,
                            }))
                            .await;
                        return;
                    }

                    // If deterministic reasoner didn't match, check if we have active cached property
                    // and user is asking an edge question about it (not asking for a different location or explicit new search)
                    if let Some(listing) = self.get_active_listing() {
                        if !AgentContextReasoner::is_different_location_or_new_search(
                            &text, listing,
                        ) {
                            let context_desc = format!(
                                "Active Property: {} in {} (ID: {}). Type: {}. Price: AED {}. Bedrooms: {}. Bathrooms: {}. Area: {} sqft. Pet Friendly: {}. Near Metro: {}. Description: {}. Available Slots: {:?}.",
                                listing.title, listing.location, listing.id, listing.listing_type, listing.price_aed, listing.bedrooms, listing.bathrooms, listing.area_sqft, listing.pet_friendly, listing.near_metro, listing.description, listing.available_slots
                            );
                            let edge_prompt = cfg
                                .agent_edge_prompt_template
                                .replace("__SYSTEM_PROMPT__", &cfg.agent_system_prompt)
                                .replace("__PROPERTY_CONTEXT__", &context_desc)
                                .replace(
                                    "__UNRELATED_QUERY_MESSAGE__",
                                    &cfg.unrelated_query_message,
                                );

                            let llm_reply =
                                self.call_pluggable_decision_llm(&edge_prompt, &text).await;

                            if let Some(edge_response) = llm_reply {
                                println!("[LlmAgentActor] Fast pluggable model answered edge question from active property context for run {}", run_id.0);

                                self.publish_assistant_conversation("LLM_RESPONSE", json!({
                                    "response_text": edge_response, "source": "llm_cached_context"
                                }).to_string()).await;

                                let _ = self.pubsub.publish(SystemEvent::VoiceTtsStream {
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
                                        result: edge_response,
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

                    let _ = self
                        .orch_tx
                        .send(QueueEvent::Agent(AgentEvent::ToolRequested {
                            run_id: run_id.clone(),
                            task_id,
                            tool_name: "book_viewing_slot".to_string(),
                            args: json!({
                                "listing_id": listing_id,
                                "user_id": user_id,
                                "slot_time": self.get_active_listing().and_then(|l| l.available_slots.first()).map(String::as_str).unwrap_or_default()
                            }),
                        }))
                        .await;
                } else {
                    let _ = self
                        .orch_tx
                        .send(QueueEvent::Agent(AgentEvent::ToolRequested {
                            run_id: run_id.clone(),
                            task_id,
                            tool_name: "search_properties".to_string(),
                            args: json!({ "query": text }),
                        }))
                        .await;
                }
            }

            AgentCmd::ToolResult {
                run_id,
                task_id,
                outcome,
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

                        let speech_text = result_value
                            .get("response_text")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Action completed successfully.")
                            .to_string();

                        self.publish_assistant_conversation("LLM_RESPONSE", json!({
                            "response_text": speech_text, "task_id": task_id.0,
                        }).to_string()).await;

                        let _ = self.pubsub.publish(SystemEvent::VoiceTtsStream {
                            call_id: run_id.call_id().to_string(),
                            audio_chunk_base64: format!("audio_chunk_{}", speech_text.len()),
                            index: 0,
                            is_end: true,
                        });

                        let _ = self
                            .orch_tx
                            .send(QueueEvent::Agent(AgentEvent::Finished {
                                run_id: run_id.clone(),
                                result: speech_text,
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
                println!(
                    "[LlmAgentActor] Run {}: Task {} is retrying (Attempt {})",
                    run_id.0, task_id.0, attempt
                );
            }

            AgentCmd::Cancel { run_id } => {
                if run_id != self.run_id {
                    return;
                }
                println!("[LlmAgentActor] Cancelled run {}", run_id.0);
            }
        }
    }
}
