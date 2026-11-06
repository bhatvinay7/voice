use super::orchestrator::Orchestrator;
use crate::models::*;
use futures_util::FutureExt;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

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
    pub agent_tx: Option<mpsc::Sender<AgentCmd>>,
    pub sts_tx: Option<mpsc::Sender<StsPipelineCmd>>,
    pub s3_dump_tx: Option<mpsc::Sender<S3DumpData>>,
    pub conversation_tx: tokio::sync::broadcast::Sender<ConversationEvent>,
    pub tts_audio_tx: Option<mpsc::Sender<TtsAudioPacket>>,
    pub audio_broadcast_tx: tokio::sync::broadcast::Sender<(String, Vec<u8>)>,
    pub ai_listen_muted: bool,
    pub last_user_transcript: Option<(String, std::time::Instant)>,
    pub cancel_token: CancellationToken,
    pub orch: Orchestrator,
    /// Holds the last user turn text until the agent reports back.
    /// When the agent finishes as `Relevant` this is flushed to Redis.
    /// When `Irrelevant` it is discarded from Redis (still in S3 via the
    /// S3 fanout subscriber which received it immediately).
    pub pending_user_turn: Option<String>,
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
    /// Version accepted by the graph reconciler for the next response event.
    /// The mailbox re-checks it before speech so a newer tool update wins.
    pub pending_graph_response_version: Option<u64>,
    /// A confirmed barge-in revokes the pending assistant completion. It is
    /// reset only after the next accepted user turn begins planning.
    pub suppress_agent_response_until_next_user_turn: bool,
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

        let (audio_broadcast_tx, mut tone_rx) =
            tokio::sync::broadcast::channel::<(String, Vec<u8>)>(100);
        let agent_tx_clone = agent_tx.clone();
        let r_id = run_id.clone();

        tokio::spawn(async move {
            let mut chunk_count = 0;
            let cfg =
                config::AppConfig::load().unwrap_or_else(|_| config::AppConfig::load_default());
            let tone_api_key = cfg.tone_model_api_key.clone();

            while let Ok((sess_id, _audio_chunk)) = tone_rx.recv().await {
                chunk_count += 1;
                if chunk_count % 5 == 0 {
                    let agent_tx_inner = agent_tx_clone.clone();
                    let rid_inner = r_id.clone();
                    let _api_key = tone_api_key.clone();
                    let s_id = sess_id.clone();

                    tokio::spawn(async move {
                        let fut = std::panic::AssertUnwindSafe(async {
                            let _ = agent_tx_inner
                                .send(AgentCmd::InjectToneContext {
                                    run_id: rid_inner.clone(),
                                    session_id: s_id,
                                    tone_intent: "neutral".to_string(),
                                    confidence: 0.85,
                                })
                                .await;
                        });

                        if let Err(_) = fut.catch_unwind().await {
                            eprintln!(
                                "[ToneModel] Panic intercepted during tone inference for run {}",
                                rid_inner.0
                            );
                        }
                    });
                }
            }
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
            agent_tx: Some(agent_tx),
            sts_tx,
            s3_dump_tx: Some(s3_tx),
            conversation_tx,
            tts_audio_tx: Some(tts_audio_tx),
            audio_broadcast_tx,
            ai_listen_muted: false,
            last_user_transcript: None,
            cancel_token: CancellationToken::new(),
            orch,
            pending_user_turn: None,
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
            pending_graph_response_version: None,
            suppress_agent_response_until_next_user_turn: false,
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

    pub async fn publish_graph_state(&self) {
        let Some(user_id) = self.conversation_graph.latest_user_node.as_ref() else {
            return;
        };
        let Some(user_node) = self.conversation_graph.nodes.get(user_id) else {
            return;
        };
        let query_id = user_node.event_id.clone();
        let query_created_at = user_node.created_at.clone();
        // This is a lifecycle checkpoint, not an inference payload.  Keeping
        // snapshots out of Sled avoids routing a large graph through the
        // orchestrator merely because relation state changed.
        let payload = serde_json::json!({
            "query_id": query_id,
            "query_created_at": query_created_at,
            "graph_version": self.conversation_graph.graph_version,
            "pending_node_count": self.conversation_graph.pending_llm_node_ids().len(),
        });
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
        self.cancel_token.cancel();
        self.orch.phone_media.close(&self.session_id);
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
