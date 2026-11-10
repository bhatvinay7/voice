use super::orchestrator::Orchestrator;
use crate::models::*;
use dashmap::DashMap;
use futures_util::FutureExt;
use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub struct PendingClientTranscript {
    pub text: String,
    pub language: String,
}

pub struct SessionActor {
    pub session_id: String,
    pub user_id: String,
    pub run_id: RunId,
    pub tasks: HashMap<TaskId, TaskStateRecord>,
    /// Cancellation is scoped to a task and owned by this session. A slow tool
    /// can never hold another session's work hostage.
    pub task_cancellations: HashMap<TaskId, CancellationToken>,
    pub db_tasks: HashMap<TaskId, (usize, PendingDbAction)>,
    pub workflow_graph: Option<workflow::SessionWorkflow>,
    /// Conversation graph is separate from the durable workflow definition.
    /// It stores user text, optimized intent, tool context, and responses for
    /// the secondary response/reconstruction path.
    pub conversation_graph: workflow::ConversationGraph,
    /// Small, mutable working memory owned by this actor.  The graph remains
    /// the audit trail; this state is the deterministic follow-up resolver.
    pub session_memory: workflow::SessionMemory,
    /// Session-local, graph-worker-owned index. Agent reads are served as
    /// lightweight references and then resolve exact nodes from this actor's
    /// conversation graph. Agent commands never receive mutation capability.
    pub graph_node_references: DashMap<String, workflow::ConversationNodeReference>,
    pub agent_tx: Option<mpsc::Sender<AgentCmd>>,
    pub sts_tx: Option<mpsc::Sender<StsPipelineCmd>>,
    pub s3_dump_tx: Option<mpsc::Sender<S3DumpData>>,
    pub conversation_tx: tokio::sync::broadcast::Sender<ConversationEvent>,
    pub tts_audio_tx: Option<mpsc::Sender<TtsAudioPacket>>,
    pub ai_listen_muted: bool,
    pub last_user_transcript: Option<(String, std::time::Instant)>,
    pub cancel_token: CancellationToken,
    pub orch: Orchestrator,
    /// Holds the last user turn text until the agent reports back.
    /// When the agent finishes as `Relevant` this is flushed to Redis.
    /// When `Irrelevant` it is discarded from Redis (still in S3 via the
    /// S3 fanout subscriber which received it immediately).
    pub pending_user_turn: Option<String>,
    /// Browser text can arrive as several final-looking packets. Keep it out
    /// of the graph until the client has been quiet for the debounce window.
    pub pending_client_transcript: Option<PendingClientTranscript>,
    pub client_transcript_generation: u64,
    /// Session-local reactive ordering state. Worker tasks only return typed
    /// events to this mailbox.
    pub transcript_reorder: workflow::ReorderBuffer,
    pub output_sequencer: workflow::OutputSequencer,
    pub tool_registry: workflow::ToolRegistry,
    pub speech_turn_id: u64,
    pub speech_generation_id: u64,
    /// True when the active STS/TTS pipeline is not currently speaking.
    pub sts_idle: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub pending_query_id: Option<String>,
    pub pending_query_text: Option<String>,
    pub pending_query_clean: Option<String>,
    pub pending_query_noise: Option<crate::audio_gate::NoiseProfile>,
    pub pending_query_started_at: Option<std::time::Instant>,
    /// Last non-final ASR update. The fast planner is held until this stream
    /// has been quiet for the configured debounce interval.
    pub last_sts_partial_at: Option<std::time::Instant>,
    /// Version accepted by the graph reconciler for the next response event.
    /// The mailbox re-checks it before speech so a newer tool update wins.
    pub pending_graph_response_version: Option<u64>,
    /// Completed main-agent output held while the caller has the floor.
    pub deferred_agent_response: Option<DeferredAgentResponse>,
    /// Graph response node whose audio is currently being emitted through STS.
    pub active_speech_response_node_id: Option<String>,
    pub active_speech_query_node_id: Option<String>,
    /// Interruption awaiting transcript confirmation or a false-positive verdict.
    pub pending_interruption_generation_id: Option<u64>,
    /// False-positive verification already scheduled for this interruption.
    pub interruption_verification_generation_id: Option<u64>,
    /// Verification reached its retry limit while speech was active; resume
    /// checking only after a later caller-silence event.
    pub interruption_verification_waiting_generation_id: Option<u64>,
    /// A confirmed barge-in revokes the pending assistant completion. It is
    /// reset only after the next accepted user turn begins planning.
    pub suppress_agent_response_until_next_user_turn: bool,
    /// Session-local LLM speaker priority flags. User speech promotes the
    /// fast planner and immediately revokes main-agent output permission.
    pub fast_llm_active: bool,
    pub main_llm_active: bool,
    /// True while VAD/STT has detected caller speech. This is persisted so a
    /// late main-model completion cannot send audio during a barge-in.
    pub user_speaking: bool,
    /// Latest graph revision dispatched to the reconstruction loop. Sled's
    /// recovery watcher may repeat the same write, so the live actor dedupes it.
    pub last_graph_request_version: u64,
}

impl SessionActor {
    /// Broadcast a conversation event to all subscribers (S3 + Redis).
    /// Both subscribers receive every event; only the Redis fanout respects
    /// the `relevance` tag (it skips `Irrelevant` events).
    pub fn publish_conversation_with_relevance(
        &self,
        event_type: impl Into<String>,
        payload_json: String,
        relevance: ConversationRelevance,
    ) {
        let event = ConversationEvent {
            session_id: self.session_id.clone(),
            trace_id: format!("trace-{}", chrono::Utc::now().timestamp_millis()),
            event_type: event_type.into(),
            payload_json,
            relevance,
        };
        if let Err(error) = self.conversation_tx.send(event) {
            tracing::warn!(%self.session_id, %error, "Conversation fanout has no active subscribers");
        }
    }

    /// Convenience wrapper — always `Relevant` (default, unchanged call sites).
    pub fn publish_conversation(&self, event_type: impl Into<String>, payload_json: String) {
        self.publish_conversation_with_relevance(
            event_type,
            payload_json,
            ConversationRelevance::Relevant,
        );
    }

    pub fn new(session_id: String, orch: Orchestrator) -> Self {
        let run_id = RunId(format!("run-{}", session_id));
        let text_jitter_ms = config::AppConfig::load()
            .unwrap_or_else(|_| config::AppConfig::load_default())
            .sts_text_jitter_ms;
        let (conversation_tx, _) = tokio::sync::broadcast::channel::<ConversationEvent>(256);
        let agent_tx = (orch.agent_factory)(run_id.clone(), orch.queue_tx.clone());
        let memory_key = format!("session:{}:memory", session_id);
        let session_memory = orch
            .local_db
            .get(memory_key.as_bytes())
            .ok()
            .flatten()
            .and_then(|bytes| serde_json::from_slice::<workflow::SessionMemory>(&bytes).ok())
            .unwrap_or_default();

        let (s3_tx, s3_rx) = mpsc::channel(1_000);
        let s_id = session_id.clone();
        tokio::spawn(async move {
            crate::supervisors::run_s3_dump_pipeline(s_id, s3_rx).await;
        });

        let s3_conversation_tx = s3_tx.clone();
        let s3_conversation_rx = conversation_tx.subscribe();
        let s3_conversation_session = session_id.clone();
        tokio::spawn(async move {
            crate::supervisors::run_s3_conversation_fanout(
                s3_conversation_session,
                s3_conversation_rx,
                s3_conversation_tx,
            )
            .await;
        });

        let redis_conversation_rx = conversation_tx.subscribe();
        let redis_conversation_session = session_id.clone();
        let redis_db = orch.db.clone();
        tokio::spawn(async move {
            crate::supervisors::run_redis_conversation_fanout(
                redis_conversation_session,
                redis_conversation_rx,
                redis_db,
            )
            .await;
        });

        let _ = agent_tx.try_send(AgentCmd::SetConversationFanout {
            run_id: run_id.clone(),
            conversation_tx: conversation_tx.clone(),
        });

        let (tts_audio_tx, tts_audio_rx) = mpsc::channel(256);
        let tts_session_id = session_id.clone();
        tokio::spawn(async move {
            crate::supervisors::run_tts_audio_packet_pipeline(tts_session_id, tts_audio_rx).await;
        });

        let (sts_event_tx, mut sts_event_rx) = mpsc::channel(100);

        // ── Pipeline selection ──────────────────────────────────────────────────
        //  STS_MODE=STS_NORMAL    → standard Whisper STT → LLM → OpenAI TTS
        //  STS_MODE=STS_STREAM    → streaming gpt-4o-realtime WebSocket (audio+text)
        let sts_mode = config::AppConfig::load_default().sts_mode;

        let sts_tx = if session_id.starts_with("phone:") {
            // Phone calls have a dedicated media transport; skip LiveKit entirely.
            orch.phone_media.sender(&session_id).map(|output| {
                crate::phone_pipeline::spawn(
                    session_id.clone(),
                    sts_event_tx,
                    s3_tx.clone(),
                    output,
                )
            })
        } else if sts_mode.is_realtime() {
            tracing::info!(
                session = %session_id,
                "STS pipeline: STS_STREAM (gpt-4o-realtime WebSocket)"
            );
            Some(
                crate::realtime_pipe::RealtimePipelineWorker::spawn(
                    session_id.clone(),
                    sts_event_tx,
                    s3_tx.clone(),
                )
                .get_cmd_tx(),
            )
        } else {
            tracing::info!(
                session = %session_id,
                "STS pipeline: STS_NORMAL (Whisper STT + OpenAI TTS)"
            );
            Some(
                crate::sts_pipe::StsPipelineWorker::spawn(
                    session_id.clone(),
                    sts_event_tx,
                    s3_tx.clone(),
                )
                .get_cmd_tx(),
            )
        };

        let q_tx_inner = orch.queue_tx.clone();
        let session_id_clone = session_id.clone();
        tokio::spawn(async move {
            while let Some(event) = sts_event_rx.recv().await {
                let _ = q_tx_inner
                    .send(QueueEvent::StsEvent {
                        session_id: session_id_clone.clone(),
                        event,
                    })
                    .await;
            }
        });

        Self {
            run_id,
            session_id: session_id.clone(),
            user_id: String::new(),
            tasks: HashMap::new(),
            task_cancellations: HashMap::new(),
            db_tasks: HashMap::new(),
            workflow_graph: None,
            conversation_graph: workflow::ConversationGraph::new(session_id.clone()),
            session_memory,
            graph_node_references: DashMap::new(),
            agent_tx: Some(agent_tx),
            sts_tx,
            s3_dump_tx: Some(s3_tx),
            conversation_tx,
            tts_audio_tx: Some(tts_audio_tx),
            ai_listen_muted: false,
            last_user_transcript: None,
            cancel_token: CancellationToken::new(),
            orch,
            pending_user_turn: None,
            pending_client_transcript: None,
            client_transcript_generation: 0,
            transcript_reorder: workflow::ReorderBuffer::new(
                0,
                Duration::from_millis(text_jitter_ms),
            ),
            output_sequencer: workflow::OutputSequencer::default(),
            tool_registry: workflow::ToolRegistry::default(),
            speech_turn_id: 0,
            speech_generation_id: 0,
            sts_idle: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
            pending_query_id: None,
            pending_query_text: None,
            pending_query_clean: None,
            pending_query_noise: None,
            pending_query_started_at: None,
            last_sts_partial_at: None,
            pending_graph_response_version: None,
            deferred_agent_response: None,
            active_speech_response_node_id: None,
            active_speech_query_node_id: None,
            pending_interruption_generation_id: None,
            interruption_verification_generation_id: None,
            interruption_verification_waiting_generation_id: None,
            suppress_agent_response_until_next_user_turn: false,
            fast_llm_active: false,
            main_llm_active: false,
            user_speaking: false,
            last_graph_request_version: 0,
        }
    }

    pub async fn enqueue_tts_audio_packet(
        &self,
        packet: TtsAudioPacket,
    ) -> Result<(), mpsc::error::SendError<TtsAudioPacket>> {
        if let Some(tx) = &self.tts_audio_tx {
            tx.send(packet).await?;
        }
        Ok(())
    }

    pub async fn emit_ordered_speech(&mut self, text: String) {
        self.sts_idle
            .store(false, std::sync::atomic::Ordering::SeqCst);
        self.speech_generation_id = self.speech_generation_id.saturating_add(1);
        let generation_id = self.speech_generation_id;
        // Feed STS phrase-sized units. Realtime/TTS transports may emit very
        // small deltas, but sending every short sentence immediately makes
        // speech sound clipped (and produces logs/UI updates of only a couple
        // of words). Keep sentence boundaries while coalescing short units.
        let raw_segments = text
            .split_inclusive(|c: char| matches!(c, '.' | '!' | '?' | '\n'))
            .filter(|segment| !segment.trim().is_empty())
            .map(str::trim)
            .map(str::to_string)
            .collect::<Vec<_>>();
        let mut segments = Vec::new();
        let mut pending = String::new();
        for segment in raw_segments {
            if !pending.is_empty() {
                pending.push(' ');
            }
            pending.push_str(&segment);
            let words = pending.split_whitespace().count();
            let sentence_complete = segment
                .chars()
                .last()
                .map(|last| matches!(last, '.' | '!' | '?' | '\n'))
                .unwrap_or(false);
            // A short acknowledgement can go out immediately, otherwise wait
            // for a useful phrase (roughly 8 words / 48 chars).
            if (sentence_complete && (words >= 8 || pending.len() >= 48)) || pending.len() >= 96 {
                segments.push(std::mem::take(&mut pending));
            }
        }
        if !pending.trim().is_empty() {
            segments.push(pending);
        }
        let segments = if segments.is_empty() {
            vec![text]
        } else {
            segments
        };
        tracing::info!(
            session = %self.session_id,
            generation_id,
            segments = segments.len(),
            "delivering agent response to STS"
        );
        for (output_sequence, segment) in segments.into_iter().enumerate() {
            let released = self.output_sequencer.push(workflow::SpeechSegment {
                turn_id: self.speech_turn_id,
                generation_id,
                output_sequence: output_sequence as u64,
                text: segment,
                stable: true,
            });
            if let Some(sts_tx) = &self.sts_tx {
                for chunk in released {
                    if let Err(error) = sts_tx
                        .send(crate::models::StsPipelineCmd::InjectAgentContext {
                            text: chunk.text,
                        })
                        .await
                    {
                        tracing::warn!(session = %self.session_id, %error, "ordered speech delivery stopped");
                        return;
                    }
                }
            } else {
                tracing::error!(
                    session = %self.session_id,
                    generation_id,
                    "agent response could not be spoken: STS pipeline sender is unavailable"
                );
                self.sts_idle
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                return;
            }
        }
        self.speech_turn_id = self.speech_turn_id.saturating_add(1);
    }

    /// Persist the fast/main LLM speaker priority only for this session.
    /// The in-memory fields gate the SessionActor mailbox; Sled mirrors them
    /// so interrupted/recovered actors cannot inherit stale speaker state.
    pub fn set_llm_speaker_state(&mut self, fast_active: bool, main_active: bool) {
        let previous = (self.fast_llm_active, self.main_llm_active);
        // Mutual exclusion invariant: a fast request and a main-agent speech
        // turn can never own the session transport simultaneously.
        if fast_active && main_active {
            self.fast_llm_active = true;
            self.main_llm_active = false;
        } else {
            self.fast_llm_active = fast_active;
            self.main_llm_active = main_active;
        }
        if previous != (self.fast_llm_active, self.main_llm_active) {
            tracing::info!(
                session = %self.session_id,
                previous_fast = previous.0,
                previous_main = previous.1,
                fast_llm_active = self.fast_llm_active,
                main_llm_active = self.main_llm_active,
                "LLM speaker ownership changed"
            );
        }
        if let Ok(tree) = self.orch.local_db.open_tree("session_runtime") {
            let base = format!("session:{}:llm_speaker", self.session_id);
            let _ = tree.insert(
                format!("{}:fast", base).as_bytes(),
                self.fast_llm_active.to_string().as_bytes(),
            );
            let _ = tree.insert(
                format!("{}:main", base).as_bytes(),
                self.main_llm_active.to_string().as_bytes(),
            );
            let _ = tree.insert(
                format!("{}:user_speaking", base).as_bytes(),
                self.user_speaking.to_string().as_bytes(),
            );
        }
    }

    pub fn set_user_speaking(&mut self, speaking: bool) {
        self.user_speaking = speaking;
        if let Ok(tree) = self.orch.local_db.open_tree("session_runtime") {
            let key = format!("session:{}:llm_speaker:user_speaking", self.session_id);
            let _ = tree.insert(key.as_bytes(), speaking.to_string().as_bytes());
        }
        tracing::info!(
            session = %self.session_id,
            user_speaking = speaking,
            fast_llm_active = self.fast_llm_active,
            main_llm_active = self.main_llm_active,
            "caller speech ownership changed"
        );
    }

    pub fn main_llm_may_emit(&self) -> bool {
        self.main_llm_active
            && !self.fast_llm_active
            && !self.user_speaking
            && !self.suppress_agent_response_until_next_user_turn
    }

    pub async fn record_speech_interruption(&mut self, generation_id: u64, timestamp_ms: u64) {
        self.pending_interruption_generation_id = Some(generation_id);
        if self.interruption_verification_generation_id != Some(generation_id) {
            self.interruption_verification_generation_id = None;
        }
        if self.interruption_verification_waiting_generation_id != Some(generation_id) {
            self.interruption_verification_waiting_generation_id = None;
        }
        let response_node_id = self.active_speech_response_node_id.clone().or_else(|| {
            self.deferred_agent_response
                .as_ref()
                .map(|response| response.response_node_id.clone())
        });
        let query_node_id = self
            .active_speech_query_node_id
            .clone()
            .or_else(|| {
                self.deferred_agent_response
                    .as_ref()
                    .and_then(|response| response.query_node_id.clone())
            })
            .or_else(|| {
                if response_node_id.is_none() {
                    self.conversation_graph.latest_user_node.clone()
                } else {
                    None
                }
            });
        if self
            .conversation_graph
            .append_assistant_interrupted_for_nodes(
                generation_id,
                timestamp_ms,
                response_node_id.as_deref(),
                query_node_id.as_deref(),
            )
            .is_some()
        {
            self.publish_graph_state().await;
        }
    }

    pub fn clear_llm_speaker_state(&mut self) {
        self.set_llm_speaker_state(false, false);
        if let Ok(tree) = self.orch.local_db.open_tree("session_runtime") {
            let base = format!("session:{}:llm_speaker", self.session_id);
            let _ = tree.remove(format!("{}:fast", base).as_bytes());
            let _ = tree.remove(format!("{}:main", base).as_bytes());
            let _ = tree.remove(format!("{}:user_speaking", base).as_bytes());
        }
    }

    /// Graph-worker reconciliation: copy lightweight, context-aware node
    /// references into the session-local DashMap. Full node bodies remain only
    /// in ConversationGraph; the map is never exposed as a mutation interface.
    pub fn clear_session_runtime_state(&mut self) {
        self.clear_llm_speaker_state();
        self.conversation_graph.reset();
        self.session_memory = workflow::SessionMemory::default();
        self.graph_node_references.clear();
        self.pending_user_turn = None;
        self.pending_client_transcript = None;
        self.client_transcript_generation = self.client_transcript_generation.saturating_add(1);
        self.pending_query_id = None;
        self.pending_query_text = None;
        self.pending_query_clean = None;
        self.pending_query_noise = None;
        self.pending_query_started_at = None;
        self.deferred_agent_response = None;
        self.active_speech_response_node_id = None;
        self.active_speech_query_node_id = None;
        self.pending_interruption_generation_id = None;
        self.interruption_verification_generation_id = None;
        self.interruption_verification_waiting_generation_id = None;
        self.last_user_transcript = None;
        self.suppress_agent_response_until_next_user_turn = false;

        let prefixes = vec![
            format!("session:{}", self.session_id),
            format!("timeout:{}", self.session_id),
            format!("session:deleted:{}", self.session_id),
        ];
        for prefix in prefixes {
            let keys = self
                .orch
                .local_db
                .scan_prefix(prefix.as_bytes())
                .filter_map(|entry| entry.ok().map(|(key, _)| key))
                .collect::<Vec<_>>();
            for key in keys {
                let _ = self.orch.local_db.remove(key);
            }
        }
        let exact_keys = [
            format!("session:{}", self.session_id),
            format!("session:deleted:{}", self.session_id),
        ];
        for key in exact_keys {
            let _ = self.orch.local_db.remove(key.as_bytes());
        }
        let _ = self.orch.local_db.flush();
        tracing::info!(session = %self.session_id, "cleared SessionActor graph, DashMap references, and Sled session state");
    }

    /// Refresh memory from the graph and persist it before handing context to
    /// an LLM. A malformed tool payload or an unexpected panic must preserve
    /// the previous valid memory rather than fail the SessionActor.
    pub fn refresh_session_memory(&mut self) {
        let graph = self.conversation_graph.clone();
        let memory = match catch_unwind(AssertUnwindSafe(|| {
            workflow::SessionMemory::from_conversation_graph(&graph)
        })) {
            Ok(memory) => memory,
            Err(_) => {
                tracing::error!(session = %self.session_id, "session memory refresh panicked; retaining previous memory");
                return;
            }
        };
        let key = format!("session:{}:memory", self.session_id);
        match serde_json::to_vec(&memory) {
            Ok(encoded) => {
                if let Err(error) = self.orch.local_db.insert(key.as_bytes(), encoded) {
                    tracing::warn!(session = %self.session_id, %error, "could not persist session memory to Sled");
                }
            }
            Err(error) => {
                tracing::warn!(session = %self.session_id, %error, "could not serialize session memory");
            }
        }
        self.session_memory = memory;
    }

    /// Validate an LLM enrichment proposal against actor-owned evidence before
    /// it is admitted to the graph. LLM output can suggest state but can never
    /// invent a node or property/entity identity.
    pub fn validate_graph_memory_enrichment(&mut self, raw: &str) -> Option<String> {
        let raw = raw
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();
        let proposal = serde_json::from_str::<workflow::GraphMemoryEnrichment>(raw).ok()?;
        if proposal.schema_version != 1
            || proposal
                .topic_summary
                .as_ref()
                .is_some_and(|summary| summary.chars().count() > 600)
            || proposal
                .referenced_node_ids
                .iter()
                .any(|id| !self.conversation_graph.nodes.contains_key(id))
        {
            return None;
        }
        self.refresh_session_memory();
        let valid_entities = self
            .session_memory
            .active_entity_ids
            .iter()
            .collect::<std::collections::BTreeSet<_>>();
        let entity_is_valid = |entity: &str| valid_entities.contains(&entity.to_string());
        if proposal
            .active_entity_id
            .as_deref()
            .is_some_and(|id| !entity_is_valid(id))
            || proposal
                .active_entity_ids
                .iter()
                .any(|id| !entity_is_valid(id))
            || proposal.ordinal_map.values().any(|id| !entity_is_valid(id))
            || proposal
                .follow_up_aliases
                .values()
                .any(|id| !entity_is_valid(id))
        {
            return None;
        }
        serde_json::to_string(&proposal).ok()
    }

    pub fn sync_graph_node_references(&self) {
        let live_ids: std::collections::BTreeSet<String> =
            self.conversation_graph.nodes.keys().cloned().collect();
        self.graph_node_references
            .retain(|node_id, _| live_ids.contains(node_id));
        for (node_id, node) in &self.conversation_graph.nodes {
            self.graph_node_references.insert(
                node_id.clone(),
                workflow::ConversationNodeReference::from_node(node),
            );
        }
    }

    /// Read-only Agent context lookup. Selects references from the session
    /// DashMap, then resolves those references to bounded graph nodes.
    pub fn read_graph_reference_context(
        &mut self,
        query_id: &str,
        requested_node_ids: &[String],
        include_connected_nodes: bool,
    ) -> Option<String> {
        self.refresh_session_memory();
        self.sync_graph_node_references();
        let snapshot = self
            .conversation_graph
            .relevant_snapshot_json_for_query(query_id)?;
        let snapshot_value = serde_json::from_str::<serde_json::Value>(&snapshot).ok()?;
        let connected_ids = snapshot_value["nodes"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|node| node.get("id")?.as_str().map(str::to_string))
            .collect::<std::collections::BTreeSet<_>>();

        let mut references = connected_ids
            .iter()
            .filter_map(|node_id| {
                self.graph_node_references
                    .get(node_id)
                    .map(|entry| entry.value().clone())
            })
            .collect::<Vec<_>>();
        references.sort_by(|left, right| {
            right
                .turn_id
                .cmp(&left.turn_id)
                .then_with(|| right.graph_version.cmp(&left.graph_version))
                .then_with(|| left.node_id.cmp(&right.node_id))
        });
        references.truncate(64);
        let relations = snapshot_value["relations"].clone();
        let mut context = serde_json::json!({
            "graph_version": self.conversation_graph.graph_version,
            "query_id": query_id,
            "latest_user_node": snapshot_value["latest_user_node"].clone(),
            "latest_context_node": snapshot_value["latest_context_node"].clone(),
            "memory": self.session_memory,
            "node_references": references,
            "relations": relations,
            "resolved_nodes": [],
        });

        let interrupted_responses = connected_ids
            .iter()
            .filter_map(|node_id| self.conversation_graph.nodes.get(node_id))
            .filter(|node| {
                node.interruption_state
                    .as_ref()
                    .is_some_and(|state| state.replay_prompt_pending)
            })
            .cloned()
            .collect::<Vec<_>>();
        let mut interruption_related_ids = std::collections::BTreeSet::new();
        for response in &interrupted_responses {
            interruption_related_ids.insert(response.id.clone());
            for parent_id in &response.parent_ids {
                if self
                    .conversation_graph
                    .nodes
                    .get(parent_id)
                    .is_some_and(|node| node.kind == workflow::ConversationNodeKind::ToolContext)
                {
                    interruption_related_ids.insert(parent_id.clone());
                }
            }
            interruption_related_ids.extend(
                connected_ids
                    .iter()
                    .filter(|node_id| {
                        self.conversation_graph
                            .nodes
                            .get(*node_id)
                            .is_some_and(|node| {
                                node.kind == workflow::ConversationNodeKind::AssistantInterrupted
                                    && node.parent_ids.contains(&response.id)
                            })
                    })
                    .cloned(),
            );
        }

        let resolved = if include_connected_nodes {
            let mut seen = std::collections::BTreeSet::new();
            let mut ids = interruption_related_ids
                .into_iter()
                .filter(|node_id| seen.insert(node_id.clone()))
                .collect::<Vec<_>>();
            let mut recent = connected_ids.iter().cloned().collect::<Vec<_>>();
            recent.sort_by_key(|node_id| {
                std::cmp::Reverse(
                    self.conversation_graph
                        .nodes
                        .get(node_id)
                        .map_or(0, |node| node.graph_version),
                )
            });
            ids.extend(
                recent
                    .into_iter()
                    .filter(|node_id| seen.insert(node_id.clone())),
            );
            ids.into_iter()
                .take(24)
                .filter_map(|node_id| self.conversation_graph.nodes.get(&node_id).cloned())
                .collect::<Vec<_>>()
        } else if !requested_node_ids.is_empty() {
            let mut seen = std::collections::BTreeSet::new();
            let mut ids = interruption_related_ids
                .into_iter()
                .filter(|node_id| seen.insert(node_id.clone()))
                .collect::<Vec<_>>();
            ids.extend(
                requested_node_ids
                    .iter()
                    .filter(|node_id| connected_ids.contains(*node_id))
                    .take(6)
                    .filter(|node_id| seen.insert((*node_id).clone()))
                    .cloned(),
            );
            ids.into_iter()
                .take(24)
                .filter_map(|node_id| self.conversation_graph.nodes.get(&node_id).cloned())
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        context["resolved_nodes"] = serde_json::json!(resolved);
        if !interrupted_responses.is_empty() {
            let interruption_contexts = interrupted_responses
                .iter()
                .map(|response| {
                    let linked_tool_contexts = response
                        .parent_ids
                        .iter()
                        .filter_map(|parent_id| self.conversation_graph.nodes.get(parent_id))
                        .filter(|node| node.kind == workflow::ConversationNodeKind::ToolContext)
                        .collect::<Vec<_>>();
                    serde_json::json!({
                        "response": response,
                        "linked_tool_contexts": linked_tool_contexts,
                    })
                })
                .collect::<Vec<_>>();
            context["interrupted_response_guidance"] = serde_json::json!({
                "replay_prompt_pending": true,
                "instruction": "An assistant response was interrupted before the caller heard it completely. Use the interrupted response and its linked tool context to understand what was being said. If the caller's latest message is a request to repeat, continue, or clarify that response, briefly ask whether they want the remaining answer repeated or clarified; do not claim the caller heard the cut-off content. Speak only after the caller turn is complete.",
                "responses": interruption_contexts,
            });
        }
        Some(context.to_string())
    }

    pub async fn publish_graph_state(&mut self) {
        self.refresh_session_memory();
        self.sync_graph_node_references();
        let Some(user_id) = self.conversation_graph.latest_user_node.as_ref() else {
            return;
        };
        let Some(user_node) = self.conversation_graph.nodes.get(user_id) else {
            return;
        };
        let query_id = user_node.event_id.clone();
        let query_created_at = user_node.created_at.clone();
        let graph_snapshot = self
            .conversation_graph
            .relevant_snapshot_json_for_query(&query_id)
            .and_then(|snapshot| serde_json::from_str::<serde_json::Value>(&snapshot).ok())
            .unwrap_or_else(|| serde_json::json!({}));
        // This is a lifecycle checkpoint, not an inference payload.  Keeping
        // snapshots out of Sled avoids routing a large graph through the
        // orchestrator merely because relation state changed.
        let payload = serde_json::json!({
            "session_id": self.session_id,
            "query_id": query_id,
            "query_created_at": query_created_at,
            "graph_version": self.conversation_graph.graph_version,
            "pending_node_count": self.conversation_graph.pending_llm_node_ids().len(),
            "latest_user_node": self.conversation_graph.latest_user_node.clone(),
            "latest_context_node": self.conversation_graph.latest_context_node.clone(),
            "node_count": self.conversation_graph.nodes.len(),
            "dashmap_reference_count": self.graph_node_references.len(),
            "graph_snapshot": graph_snapshot,
        });
        tracing::info!(
            session = %self.session_id,
            graph_version = self.conversation_graph.graph_version,
            graph_data = %payload,
            "in-memory conversation graph snapshot"
        );
        if let Ok(tree) = self.orch.local_db.open_tree("session_graph") {
            let key = format!(
                "session:{}:{}",
                self.session_id, self.conversation_graph.graph_version
            );
            if let Err(error) = tree.insert(key.as_bytes(), payload.to_string().as_bytes()) {
                tracing::warn!(%error, session = %self.session_id, "graph checkpoint write failed");
            }
        }
    }

    /// Stop the caller-facing generation without cancelling pending tools.
    /// Tool work is graph data: a subsequent fast plan decides whether a new
    /// user turn depends on it, rather than speech barge-in destroying it.
    pub async fn cancel_active_generation_work(&mut self) {
        self.pending_graph_response_version = None;
        if let Some(agent_tx) = &self.agent_tx {
            let _ = agent_tx
                .send(AgentCmd::CancelGeneration {
                    run_id: self.run_id.clone(),
                })
                .await;
        }
    }

    pub async fn run(mut self, mut rx: mpsc::Receiver<SessionEvent>) {
        println!("[SessionActor {}] Started.", self.session_id);
        loop {
            tokio::select! {
                Some(event) = rx.recv() => {
                    self.handle_queue_event(event).await;
                }
                _ = self.cancel_token.cancelled() => {
                    println!("[SessionActor {}] Cancel token triggered. Shutting down.", self.session_id);
                    break;
                }
                else => break,
            }
        }

        self.orch.phone_media.close(&self.session_id);
        self.clear_session_runtime_state();
        if let Some(graph) = &mut self.workflow_graph {
            graph.release();
        }
        for cancellation in self.task_cancellations.values() {
            cancellation.cancel();
        }
        self.orch.remove_session_inbox(&self.session_id);
        let _ = self
            .orch
            .local_db
            .remove(format!("session:{}", self.session_id).as_bytes());
        let _ = self.orch.local_db.flush();
        println!("[SessionActor {}] Stopped.", self.session_id);
    }

    pub fn register_timeout(&mut self, task_id: TaskId, duration: Duration) {
        let expire_time = chrono::Utc::now().timestamp_millis() + duration.as_millis() as i64;
        let db_key = format!("timeout:{}", task_id.0);
        let _ = self
            .orch
            .local_db
            .insert(db_key.as_bytes(), &expire_time.to_be_bytes());
        let _ = self.orch.local_db.flush();

        let mut wheel = self.orch.timer_wheel.lock();
        let existing_key = wheel.keys.get(&task_id).cloned();
        if let Some(key) = existing_key {
            wheel.queue.reset(&key, duration);
        } else {
            let key = wheel.queue.insert(task_id.clone(), duration);
            wheel.keys.insert(task_id.clone(), key);
        }
        self.orch
            .task_to_session
            .insert(task_id, self.session_id.clone());
    }

    pub fn clear_timeout(&mut self, task_id: &TaskId) -> bool {
        let db_key = format!("timeout:{}", task_id.0);
        let _ = self.orch.local_db.remove(db_key.as_bytes());
        let _ = self.orch.local_db.flush();
        self.orch.task_to_session.remove(task_id);

        let mut wheel = self.orch.timer_wheel.lock();
        if let Some(key) = wheel.keys.remove(task_id) {
            wheel.queue.remove(&key);
            true
        } else {
            false
        }
    }

    pub fn persist_durable_task(
        &self,
        task: &TaskStateRecord,
        status: crate::durable_scheduler::DurableTaskStatus,
        due_at_ms: i64,
    ) {
        let durable = crate::durable_scheduler::DurableTask {
            task_id: task.task_id.clone(),
            session_id: self.session_id.clone(),
            run_id: task.run_id.clone(),
            tool_name: task.tool_name.clone(),
            args: task.args.clone(),
            query_node_id: task.query_node_id.clone(),
            query_graph_version: task.query_graph_version,
            attempt: task.attempt,
            max_attempts: task.max_retry,
            retry_count: task.retry_count,
            due_at_ms,
            lease_expires_at_ms: if status == crate::durable_scheduler::DurableTaskStatus::Claimed {
                Some(due_at_ms.saturating_add(30_000))
            } else {
                None
            },
            status,
            last_error: task.error.clone(),
        };
        if let Err(error) = self.orch.durable_scheduler.schedule(&durable) {
            tracing::error!(task = %task.task_id.0, %error, "could not persist durable task state");
        }
    }

    pub async fn dispatch_db_action(
        &mut self,
        task_id: TaskId,
        action: PendingDbAction,
        is_retry: bool,
    ) {
        if !is_retry {
            self.db_tasks.insert(task_id.clone(), (0, action.clone()));
        }

        self.register_timeout(task_id.clone(), Duration::from_secs(5));

        let db_tx = self.orch.db_tx.clone();
        let q_tx = self.orch.queue_tx.clone();
        let t_id = task_id.clone();

        match action.clone() {
            PendingDbAction::GetGraphNodes {
                session_id,
                user_id,
                language,
            } => {
                let (tx, rx) = tokio::sync::oneshot::channel();
                let _ = db_tx
                    .send(db::actor::DbRequest::GetGraphNodes {
                        idempotency_key: task_id.0.clone(),
                        reply_to: tx,
                    })
                    .await;
                tokio::spawn(async move {
                    match rx.await {
                        Ok(Ok(nodes)) => {
                            let _ = q_tx
                                .send(QueueEvent::GraphNodesFetched {
                                    session_id,
                                    user_id,
                                    language,
                                    task_id: t_id,
                                    nodes,
                                })
                                .await;
                        }
                        Ok(Err(e)) => {
                            let _ = q_tx
                                .send(QueueEvent::DbActionCompleted {
                                    task_id: t_id,
                                    action: "GetGraphNodes".to_string(),
                                    success: false,
                                    error: Some(e),
                                })
                                .await;
                        }
                        Err(e) => {
                            let _ = q_tx
                                .send(QueueEvent::DbActionCompleted {
                                    task_id: t_id,
                                    action: "GetGraphNodes".to_string(),
                                    success: false,
                                    error: Some(e.to_string()),
                                })
                                .await;
                        }
                    }
                });
            }
            PendingDbAction::PushRedisStream {
                stream_name,
                session_id,
                trace_id,
                event_type,
                payload_json,
            } => {
                let (tx, rx) = tokio::sync::oneshot::channel();
                let _ = db_tx
                    .send(db::actor::DbRequest::PushRedisStream {
                        idempotency_key: task_id.0.clone(),
                        stream_name,
                        session_id,
                        trace_id,
                        event_type,
                        payload_json,
                        reply_to: tx,
                    })
                    .await;
                tokio::spawn(async move {
                    let (success, error) = match rx.await {
                        Ok(Ok(_)) => (true, None),
                        Ok(Err(e)) => (false, Some(e)),
                        Err(e) => (false, Some(e.to_string())),
                    };
                    let _ = q_tx
                        .send(QueueEvent::DbActionCompleted {
                            task_id: t_id,
                            action: "PushRedisStream".to_string(),
                            success,
                            error,
                        })
                        .await;
                });
            }
            PendingDbAction::SetJsonWithTtl {
                key,
                payload_json,
                ttl_seconds,
            } => {
                let (tx, rx) = tokio::sync::oneshot::channel();
                let _ = db_tx
                    .send(db::actor::DbRequest::SetJsonWithTtl {
                        idempotency_key: task_id.0.clone(),
                        key,
                        payload_json,
                        ttl_seconds,
                        reply_to: tx,
                    })
                    .await;
                tokio::spawn(async move {
                    let (success, error) = match rx.await {
                        Ok(Ok(_)) => (true, None),
                        Ok(Err(e)) => (false, Some(e)),
                        Err(e) => (false, Some(e.to_string())),
                    };
                    let _ = q_tx
                        .send(QueueEvent::DbActionCompleted {
                            task_id: t_id,
                            action: "SetJsonWithTtl".to_string(),
                            success,
                            error,
                        })
                        .await;
                });
            }
            PendingDbAction::SaveS3Endpoints {
                session_id,
                audio_url,
                text_url,
            } => {
                let (tx, rx) = tokio::sync::oneshot::channel();
                let _ = db_tx
                    .send(db::actor::DbRequest::SaveS3Endpoints {
                        idempotency_key: task_id.0.clone(),
                        session_id,
                        audio_url,
                        text_url,
                        reply_to: tx,
                    })
                    .await;
                tokio::spawn(async move {
                    let (success, error) = match rx.await {
                        Ok(Ok(_)) => (true, None),
                        Ok(Err(e)) => (false, Some(e)),
                        Err(e) => (false, Some(e.to_string())),
                    };
                    let _ = q_tx
                        .send(QueueEvent::DbActionCompleted {
                            task_id: t_id,
                            action: "SaveS3Endpoints".to_string(),
                            success,
                            error,
                        })
                        .await;
                });
            }
        }
    }
}
impl Drop for SessionActor {
    fn drop(&mut self) {
        // Also runs when the session future unwinds or is aborted.
        self.clear_llm_speaker_state();
        self.cancel_token.cancel();
        self.orch.phone_media.close(&self.session_id);
        self.graph_node_references.clear();
        if let Some(tx) = &self.agent_tx {
            let _ = tx.try_send(AgentCmd::Cancel {
                run_id: self.run_id.clone(),
            });
        }
        if let Some(tx) = &self.sts_tx {
            let _ = tx.try_send(StsPipelineCmd::Shutdown);
        }
    }
}
