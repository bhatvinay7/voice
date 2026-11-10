use super::super::session::{PendingClientTranscript, SessionActor};
use crate::models::*;
use futures_util::FutureExt;
use pubsub::SystemEvent;
use serde_json::json;
use std::panic::AssertUnwindSafe;
use std::time::Duration;

const INTERRUPTION_SPEECH_STATE_MAX_CHECKS: u8 = 5;
const INTERRUPTION_SPEECH_STATE_CHECK_DELAY: Duration = Duration::from_millis(150);

impl SessionActor {
    fn schedule_interruption_speech_state_check(
        &self,
        generation_id: u64,
        attempt: u8,
        consecutive_silent_checks: u8,
    ) {
        let queue_tx = self.orch.queue_tx.clone();
        let session_id = self.session_id.clone();
        tokio::spawn(async move {
            tokio::time::sleep(INTERRUPTION_SPEECH_STATE_CHECK_DELAY).await;
            if let Err(error) = queue_tx
                .send(QueueEvent::VerifyInterruptionSpeechState {
                    session_id: session_id.clone(),
                    generation_id,
                    attempt,
                    consecutive_silent_checks,
                })
                .await
            {
                tracing::error!(
                    session = %session_id,
                    generation_id,
                    attempt,
                    %error,
                    "could not schedule interruption speech-state verification"
                );
            }
        });
    }

    async fn resume_false_positive_interruption(&mut self, session_id: &str, generation_id: u64) {
        self.pending_interruption_generation_id = None;
        self.interruption_verification_generation_id = None;
        self.interruption_verification_waiting_generation_id = None;
        self.suppress_agent_response_until_next_user_turn = false;

        let interruption = self
            .conversation_graph
            .nodes
            .values()
            .find(|node| {
                node.kind == workflow::ConversationNodeKind::AssistantInterrupted
                    && node
                        .interruption_state
                        .as_ref()
                        .is_some_and(|state| state.generation_id == generation_id)
            })
            .and_then(|node| node.interruption_state.clone());
        let Some(interruption) = interruption else {
            self.set_llm_speaker_state(false, false);
            tracing::warn!(
                session = %session_id,
                generation_id,
                "false-positive interruption has no graph record; allowing active generation to finish"
            );
            return;
        };

        let query_is_current =
            interruption
                .query_node_id
                .as_deref()
                .map_or(true, |query_node_id| {
                    self.conversation_graph.latest_user_node.as_deref() == Some(query_node_id)
                });
        if !query_is_current {
            self.set_llm_speaker_state(true, false);
            tracing::info!(
                session = %session_id,
                generation_id,
                "not resuming false-positive interruption because its query was superseded"
            );
            return;
        }

        let Some(response_node_id) = interruption.response_node_id else {
            self.set_llm_speaker_state(false, false);
            tracing::info!(
                session = %session_id,
                generation_id,
                response_ready = interruption.response_ready,
                "false-positive interruption had no completed response; active model/tool work may finish normally"
            );
            return;
        };
        let Some(response_text) = self
            .conversation_graph
            .nodes
            .get(&response_node_id)
            .filter(|node| node.kind == workflow::ConversationNodeKind::AssistantResponse)
            .map(|node| node.content.clone())
        else {
            self.set_llm_speaker_state(false, false);
            tracing::warn!(
                session = %session_id,
                generation_id,
                %response_node_id,
                "interrupted response node is missing; active model/tool work may finish normally"
            );
            return;
        };

        if self
            .conversation_graph
            .mark_interruption_resumed(&response_node_id, generation_id)
        {
            self.publish_graph_state().await;
        }
        tracing::info!(
            session = %session_id,
            generation_id,
            %response_node_id,
            query_node_id = ?interruption.query_node_id.as_deref(),
            fast_llm_active = self.fast_llm_active,
            main_llm_active = true,
            "resuming interrupted assistant response after bounded caller-silence verification"
        );
        self.active_speech_response_node_id = Some(response_node_id);
        self.active_speech_query_node_id = interruption.query_node_id;
        self.deferred_agent_response = None;
        self.set_llm_speaker_state(false, true);
        self.emit_ordered_speech(format!(
            "I thought I heard an interruption, but we were discussing this, so I will continue. {response_text}"
        ))
        .await;
    }

    pub async fn handle_queue_event(&mut self, event: QueueEvent) {
        match event {
            QueueEvent::SessionCancelled { session_id } => {
                self.orch.phone_media.close(&session_id);
                self.clear_session_runtime_state();
                let deleted_key = format!("session:deleted:{}", session_id);
                let _ = self
                    .orch
                    .local_db
                    .insert(deleted_key.as_bytes(), b"deleted");
                let _ = self
                    .orch
                    .local_db
                    .remove(format!("session:{}", session_id).as_bytes());
                let _ = self.orch.local_db.flush();
                println!("[SessionActor {}] Cancel event received.", session_id);
                // S3 receives this marker through the fanout. Redis receives a
                // synchronous append below before the worker is queued.
                self.publish_conversation_with_relevance(
                    "SESSION_ENDED",
                    json!({ "status": "ended" }).to_string(),
                    ConversationRelevance::Irrelevant,
                );

                // The worker alone deletes Redis after its Neon transaction
                // commits. Append the terminal event first, then enqueue the
                // worker only after that append succeeds. This preserves the
                // append → commit → delete ordering even during a Redis outage.
                let summary_db = self.orch.db.clone();
                let summary_session = session_id.clone();
                tokio::spawn(async move {
                    let mut appended = false;
                    for attempt in 1_u8..=5 {
                        let trace_id = format!(
                            "trace-terminal-{}-{}",
                            chrono::Utc::now().timestamp_millis(),
                            attempt
                        );
                        match summary_db
                            .append_conversation_to_redis(
                                &summary_session,
                                &trace_id,
                                "SESSION_ENDED",
                                &json!({ "status": "ended" }).to_string(),
                            )
                            .await
                        {
                            Ok(_) => {
                                appended = true;
                                break;
                            }
                            Err(error) => {
                                tracing::warn!(session_id = %summary_session, attempt, %error, "terminal Redis append deferred");
                                tokio::time::sleep(Duration::from_secs(2_u64.pow(attempt as u32)))
                                    .await;
                            }
                        }
                    }
                    if !appended {
                        tracing::error!(session_id = %summary_session, "terminal Redis append exhausted retries; summary job was not queued");
                        return;
                    }
                    if let Err(error) = summary_db.enqueue_call_summary(&summary_session, 10).await
                    {
                        tracing::warn!(session_id = %summary_session, %error, "could not enqueue conversation persistence job");
                    }
                });
                if let Some(graph) = &mut self.workflow_graph {
                    graph.release();
                }

                let trace_id = format!("trace-{}", chrono::Utc::now().timestamp_millis());
                let task_id_audit = TaskId(format!("task-db-audit-{}", trace_id));
                self.dispatch_db_action(
                    task_id_audit,
                    PendingDbAction::PushRedisStream {
                        stream_name: "stream:audit".to_string(),
                        session_id: session_id.clone(),
                        trace_id: trace_id.clone(),
                        event_type: "SESSION_ENDED".to_string(),
                        payload_json: json!({ "status": "ended" }).to_string(),
                    },
                    false,
                )
                .await;

                if let Some(agent_tx) = &self.agent_tx {
                    let _ = agent_tx
                        .send(AgentCmd::Cancel {
                            run_id: self.run_id.clone(),
                        })
                        .await;
                }
                if let Some(sts_tx) = &self.sts_tx {
                    if let Err(error) = sts_tx.send(crate::models::StsPipelineCmd::Shutdown).await {
                        tracing::warn!(
                            "Session {} STS shutdown signal failed: {}",
                            session_id,
                            error
                        );
                    }
                }

                // Finalize and upload in a detached task so the session event loop can exit promptly.
                if let Some(recording_tx) = self.s3_dump_tx.clone() {
                    let db_tx = self.orch.db_tx.clone();
                    let recording_session = session_id.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(Duration::from_millis(200)).await;
                        let (reply_to, result_rx) = tokio::sync::oneshot::channel();
                        if let Err(error) = recording_tx
                            .send(S3DumpData::Finalize {
                                session_id: recording_session.clone(),
                                reply_to,
                            })
                            .await
                        {
                            tracing::error!(
                                "Session {} recording finalize enqueue failed: {}",
                                recording_session,
                                error
                            );
                            return;
                        }

                        let (audio_url, text_url) =
                            match tokio::time::timeout(Duration::from_secs(60), result_rx).await {
                                Ok(Ok(Ok(audio_url))) => audio_url,
                                Ok(Ok(Err(error))) => {
                                    tracing::error!(
                                        "Session {} recording upload failed: {}",
                                        recording_session,
                                        error
                                    );
                                    return;
                                }
                                Ok(Err(error)) => {
                                    tracing::error!(
                                        "Session {} recording worker dropped its reply: {}",
                                        recording_session,
                                        error
                                    );
                                    return;
                                }
                                Err(_) => {
                                    tracing::error!(
                                        "Session {} recording upload timed out",
                                        recording_session
                                    );
                                    return;
                                }
                            };

                        let (reply_to, result_rx) = tokio::sync::oneshot::channel();
                        if let Err(error) = db_tx
                            .send(db::actor::DbRequest::SaveS3Endpoints {
                                idempotency_key: format!("task-db-s3-{}", trace_id),
                                session_id: recording_session.clone(),
                                audio_url,
                                text_url,
                                reply_to,
                            })
                            .await
                        {
                            tracing::error!(
                                "Session {} recording URL persistence enqueue failed: {}",
                                recording_session,
                                error
                            );
                            return;
                        }
                        match tokio::time::timeout(Duration::from_secs(10), result_rx).await {
                            Ok(Ok(Ok(()))) => tracing::info!(
                                "Session {} recording URL persisted",
                                recording_session
                            ),
                            Ok(Ok(Err(error))) => tracing::error!(
                                "Session {} recording URL persistence failed: {}",
                                recording_session,
                                error
                            ),
                            Ok(Err(error)) => tracing::error!(
                                "Session {} DB actor dropped recording URL reply: {}",
                                recording_session,
                                error
                            ),
                            Err(_) => tracing::error!(
                                "Session {} recording URL persistence timed out",
                                recording_session
                            ),
                        }
                    });
                }

                self.cancel_token.cancel();
            }

            QueueEvent::SessionStarted {
                session_id,
                user_id,
                language,
            } => {
                // Fast planning owns a new session until a validated main-agent
                // answer is ready. This keeps the state deterministic instead
                // of leaving both speaker flags unset between turns.
                self.set_llm_speaker_state(true, false);
                self.set_user_speaking(false);
                self.user_id = user_id.clone();
                let task_id = TaskId(format!("task-db-{}", session_id));
                self.dispatch_db_action(
                    task_id,
                    PendingDbAction::GetGraphNodes {
                        session_id: session_id.clone(),
                        user_id: user_id.clone(),
                        language: language.clone(),
                    },
                    false,
                )
                .await;
            }

            QueueEvent::TelephonyAudio {
                session_id,
                samples,
                sample_rate,
                channels,
                elapsed_samples,
            } => {
                if let Some(recording_tx) = &self.s3_dump_tx {
                    if let Err(error) = recording_tx.try_send(S3DumpData::Audio {
                        session_id: session_id.clone(),
                        samples: samples.clone(),
                        sample_rate,
                        channels,
                        elapsed_samples,
                        speaker: AudioSpeaker::User,
                    }) {
                        tracing::warn!(%session_id, %error, "Could not enqueue phone audio for recording");
                    }
                }
            }

            QueueEvent::SttTranscript {
                session_id,
                text,
                is_final,
                noise,
                language: _,
            } => {
                use crate::speech_filter::{sanitize_transcript, CleanedInput};

                // Always write the raw STT text to S3 for audit (noise or not).
                self.publish_conversation_with_relevance(
                    "USER_PROMPT",
                    serde_json::json!({ "text": text.clone(), "role": "user", "final": is_final })
                        .to_string(),
                    ConversationRelevance::Irrelevant,
                );

                if is_final {
                    self.pending_user_turn = Some(text.clone());
                    if self.ai_listen_muted {
                        println!(
                            "[SessionActor {}] AI is muted. Discarding STT transcript: {}",
                            session_id, text
                        );
                    } else {
                        match sanitize_transcript(&text) {
                            CleanedInput::Empty => {
                                tracing::info!(
                                    "[SessionActor {}] STT transcript is pure noise — skipping LLM: '{}'",
                                    session_id, text
                                );
                                // Noise-only turns: S3 already written above; skip Redis + LLM.
                            }
                            CleanedInput::Continuation(kind) => {
                                if let Some(agent_tx) = &self.agent_tx {
                                    let _ = agent_tx
                                        .send(crate::models::AgentCmd::ContinueLast {
                                            run_id: self.run_id.clone(),
                                            kind,
                                        })
                                        .await;
                                }
                            }
                            CleanedInput::Text(clean) => {
                                tracing::info!(
                                        session = %session_id,
                                        transcript_chars = text.chars().count(),
                                        clean_chars = clean.chars().count(),
                                        "accepted final STT transcript; scheduling agent dispatch"
                                );
                                // A final transcript is a new accepted turn.
                                // Keep stale completions barred until the
                                // generation token is handed to this turn.
                                self.suppress_agent_response_until_next_user_turn = false;
                                self.set_llm_speaker_state(true, false);
                                self.set_user_speaking(false);
                                // Persist a cleaned final user turn immediately.
                                // Do not wait for a later tool/LLM response: an
                                // interrupted call must still have a transcript.
                                self.publish_conversation_with_relevance(
                                    "USER_PROMPT",
                                    serde_json::json!({ "text": clean, "role": "user", "final": true }).to_string(),
                                    ConversationRelevance::Relevant,
                                );
                                if let Some(user_node_id) =
                                    self.conversation_graph.append_user_text(&text)
                                {
                                    let _ = self
                                        .conversation_graph
                                        .append_optimized_intent(clean.clone(), &user_node_id);
                                    self.publish_graph_state().await;
                                    let query_id = self
                                        .conversation_graph
                                        .nodes
                                        .get(&user_node_id)
                                        .map(|node| node.event_id.clone())
                                        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                                    self.pending_query_id = Some(query_id.clone());
                                    self.pending_query_text = Some(text.clone());
                                    self.pending_query_clean = Some(clean);
                                    self.pending_query_noise = noise;
                                    self.pending_query_started_at = Some(std::time::Instant::now());
                                    let queue_tx = self.orch.queue_tx.clone();
                                    let session_for_commit = session_id.clone();
                                    let cfg = config::AppConfig::load()
                                        .unwrap_or_else(|_| config::AppConfig::load_default());
                                    let delay_ms =
                                        cfg.sts_text_jitter_ms.min(cfg.sts_text_max_wait_ms.max(1));
                                    tokio::spawn(async move {
                                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                                        let _ = queue_tx
                                            .send(QueueEvent::CommitTranscript {
                                                session_id: session_for_commit,
                                                query_id,
                                            })
                                            .await;
                                    });
                                }
                            }
                        }
                    }
                }
            }

            QueueEvent::ClientTranscript {
                session_id,
                text,
                language,
            } => {
                let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
                if text.is_empty() {
                    return;
                }
                let merged = std::panic::catch_unwind(AssertUnwindSafe(|| {
                    let pending =
                        self.pending_client_transcript
                            .get_or_insert(PendingClientTranscript {
                                text: String::new(),
                                language,
                            });
                    if !pending.text.is_empty() {
                        pending.text.push(' ');
                    }
                    pending.text.push_str(&text);
                    self.client_transcript_generation =
                        self.client_transcript_generation.saturating_add(1);
                    let delay_ms = config::AppConfig::load()
                        .unwrap_or_else(|_| config::AppConfig::load_default())
                        .sts_text_jitter_ms;
                    (self.client_transcript_generation, delay_ms)
                }));
                let (generation, delay_ms) = match merged {
                    Ok(values) => values,
                    Err(_) => {
                        tracing::error!(session = %session_id, "client transcript debounce panicked; dropped incomplete client turn");
                        return;
                    }
                };
                let queue_tx = self.orch.queue_tx.clone();
                tokio::spawn(async move {
                    let result = AssertUnwindSafe(async move {
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                        let _ = queue_tx
                            .send(QueueEvent::CommitClientTranscript {
                                session_id,
                                generation,
                            })
                            .await;
                    })
                    .catch_unwind()
                    .await;
                    if result.is_err() {
                        tracing::error!(generation, "client transcript debounce flush panicked");
                    }
                });
            }

            QueueEvent::CommitClientTranscript {
                session_id,
                generation,
            } => {
                if generation != self.client_transcript_generation {
                    return;
                }
                let Some(pending) = self.pending_client_transcript.take() else {
                    return;
                };
                tracing::info!(session = %session_id, generation, transcript_chars = pending.text.chars().count(), "client transcript debounce settled; dispatching one complete turn");
                let _ = self
                    .orch
                    .queue_tx
                    .send(QueueEvent::SttTranscript {
                        session_id,
                        text: pending.text,
                        language: pending.language,
                        is_final: true,
                        noise: None,
                    })
                    .await;
            }

            QueueEvent::TranscriptFragment {
                session_id,
                stream_id,
                sequence_id,
                text,
                is_final,
            } => {
                let now = std::time::Instant::now();
                let fragments = self.transcript_reorder.push(
                    workflow::TranscriptFragment {
                        stream_id,
                        sequence_id,
                        text,
                        is_final,
                    },
                    now,
                );
                if self.transcript_reorder.gap_expired(now) {
                    if let Some(missing) = self.transcript_reorder.take_gap_marker() {
                        tracing::warn!(%session_id, missing, "transcript sequence gap exceeded bounded wait");
                    }
                }
                for fragment in fragments {
                    let _ = self
                        .orch
                        .queue_tx
                        .send(QueueEvent::SttTranscript {
                            session_id: session_id.clone(),
                            text: fragment.text,
                            language: "en".to_string(),
                            is_final: fragment.is_final,
                            noise: None,
                        })
                        .await;
                }
            }

            QueueEvent::SessionStateRequested {
                session_id,
                reply_to,
            } => {
                let active_task_count = self
                    .tasks
                    .values()
                    .filter(|task| {
                        matches!(
                            task.status,
                            TaskStatus::Pending | TaskStatus::Running | TaskStatus::Retrying
                        )
                    })
                    .count();
                let running_task_count = self
                    .tasks
                    .values()
                    .filter(|task| task.status == TaskStatus::Running)
                    .count();
                let retrying_task_count = self
                    .tasks
                    .values()
                    .filter(|task| task.status == TaskStatus::Retrying)
                    .count();
                let snapshot = SessionStateSnapshot {
                    session_id,
                    graph_version: self.conversation_graph.graph_version,
                    active_task_count,
                    running_task_count,
                    retrying_task_count,
                    pending_transcript: self.pending_query_id.is_some(),
                    sts_idle: self.sts_idle.load(std::sync::atomic::Ordering::SeqCst),
                    speech_generation_id: self.speech_generation_id,
                };
                if reply_to.send(snapshot).await.is_err() {
                    tracing::debug!(session = %self.session_id, "session-state requester stopped waiting");
                }
            }

            QueueEvent::GraphContextRequested {
                session_id,
                query_id,
                node_ids,
                reply_to,
            } => {
                // The expensive serialization is deliberately demand-only.
                // Normal graph mutation, retries, and timeout events carry
                // IDs/revisions only and never pass graph data through the
                // orchestrator queue.
                if session_id != self.session_id {
                    tracing::debug!(requested = %session_id, owned = %self.session_id, "ignoring graph context request for another session");
                    return;
                }
                // The first graph lookup is the fast planner's only chance to
                // understand a follow-up.  Give it a bounded full component
                // plus the LLM-oriented memory packet; follow-up lookups with
                // explicit IDs stay narrow.
                let context =
                    self.read_graph_reference_context(&query_id, &node_ids, node_ids.is_empty());
                let context = context.map(|context| {
                    let pending_tools = self.tasks.values().filter(|task| matches!(task.status, TaskStatus::Running | TaskStatus::Retrying)).filter_map(|task| {
                        let source_query_id = task.query_id.clone()?;
                        let question = task.query_node_id.as_ref().and_then(|node_id| self.conversation_graph.nodes.get(node_id)).map(|node| node.content.clone())?;
                        Some(serde_json::json!({"query_id": source_query_id, "tool": task.tool_name, "question": question}))
                    }).collect::<Vec<_>>();
                    let mut value = serde_json::from_str::<serde_json::Value>(&context).unwrap_or_else(|_| serde_json::json!({}));
                    value["pending_tool_queries"] = serde_json::Value::Array(pending_tools);
                    value.to_string()
                });
                let response = GraphContextResponse {
                    session_id,
                    query_id,
                    graph_version: self.conversation_graph.graph_version,
                    context,
                };
                if reply_to.send(response).await.is_err() {
                    tracing::debug!(session = %self.session_id, "graph-context requester stopped waiting");
                }
            }

            QueueEvent::GraphEnrichmentReady {
                session_id,
                query_id,
                base_graph_version,
                enrichment,
            } => {
                // Only the session actor owns the graph.  A late worker may
                // annotate an older turn, but may never attach its output to a
                // missing/replaced query or roll the graph back.
                if session_id != self.session_id {
                    return;
                }
                let Some(enrichment) = self.validate_graph_memory_enrichment(&enrichment) else {
                    tracing::warn!(session = %self.session_id, %query_id, "discarded invalid graph-memory enrichment proposal");
                    return;
                };
                if self
                    .conversation_graph
                    .upsert_graph_enrichment_for_query(&query_id, base_graph_version, enrichment)
                    .is_some()
                {
                    tracing::info!(
                        session = %self.session_id,
                        %query_id,
                        base_graph_version,
                        graph_version = self.conversation_graph.graph_version,
                        node_count = self.conversation_graph.nodes.len(),
                        "graph improver worker applied enrichment"
                    );
                    self.publish_graph_state().await;
                } else {
                    tracing::warn!(
                        session = %self.session_id,
                        %query_id,
                        base_graph_version,
                        current_graph_version = self.conversation_graph.graph_version,
                        "graph improver worker result discarded as stale"
                    );
                }
            }

            QueueEvent::CommitTranscript { query_id, .. } => {
                if self.pending_query_id.as_deref() != Some(query_id.as_str()) {
                    tracing::warn!(
                        session = %self.session_id,
                        %query_id,
                        pending_query_id = ?self.pending_query_id,
                        "commit transcript ignored because its query is no longer pending"
                    );
                    return;
                }
                // Final transcript is authoritative evidence of a new caller
                // turn. Stop any old STS response before fast planning turn
                // two, but preserve pending tools and model requests. The
                // planner can relate tools through the graph, while query and
                // generation checks determine whether old output is still valid.
                self.suppress_agent_response_until_next_user_turn = false;
                self.set_user_speaking(false);
                self.deferred_agent_response = None;
                if !self.sts_idle.load(std::sync::atomic::Ordering::SeqCst) {
                    if let Some(sts_tx) = &self.sts_tx {
                        if let Err(tokio::sync::mpsc::error::TrySendError::Full(stop)) =
                            sts_tx.try_send(crate::models::StsPipelineCmd::Stop)
                        {
                            let retry_tx = sts_tx.clone();
                            tokio::spawn(async move {
                                let _ = retry_tx.send(stop).await;
                            });
                        }
                    }
                    self.sts_idle
                        .store(true, std::sync::atomic::Ordering::SeqCst);
                    self.output_sequencer
                        .cancel_generation(self.speech_turn_id, self.speech_generation_id);
                    let timestamp_ms = chrono::Utc::now().timestamp_millis().max(0) as u64;
                    self.record_speech_interruption(self.speech_generation_id, timestamp_ms)
                        .await;
                }
                let Some(text) = self.pending_query_text.take() else {
                    return;
                };
                let clean_text = self
                    .pending_query_clean
                    .take()
                    .unwrap_or_else(|| text.clone());
                let noise = self.pending_query_noise.take();
                self.pending_query_id = None;
                self.pending_query_started_at = None;
                self.set_llm_speaker_state(true, false);
                if let Some(agent_tx) = &self.agent_tx {
                    if let Err(error) = agent_tx
                        .send(AgentCmd::SetGraphQuery {
                            run_id: self.run_id.clone(),
                            query_id: query_id.clone(),
                            playback_generation: self.speech_generation_id,
                        })
                        .await
                    {
                        tracing::error!(session = %self.session_id, %error, "agent mailbox closed before graph query was dispatched");
                        return;
                    }
                    if let Err(error) = agent_tx
                        .send(AgentCmd::UserInput {
                            run_id: self.run_id.clone(),
                            user_id: self.user_id.clone(),
                            role: SpeakerRole::Candidate,
                            text,
                            clean_text,
                            noise,
                        })
                        .await
                    {
                        tracing::error!(session = %self.session_id, %error, "agent mailbox closed; final transcript was not dispatched");
                    } else {
                        tracing::info!(session = %self.session_id, query_id = %query_id, "final transcript dispatched to agent");
                    }
                } else {
                    tracing::error!(
                        session = %self.session_id,
                        %query_id,
                        "final transcript could not be dispatched because the agent channel is unavailable"
                    );
                }
            }

            QueueEvent::ScheduledTaskDue { task_id, .. } => {
                let Some(durable) = (match self.orch.durable_scheduler.load(&task_id) {
                    Ok(task) => task,
                    Err(error) => {
                        tracing::error!(task = %task_id.0, %error, "could not load claimed durable task");
                        None
                    }
                }) else {
                    return;
                };
                if durable.status != crate::durable_scheduler::DurableTaskStatus::Claimed {
                    return;
                }
                let cfg =
                    config::AppConfig::load().unwrap_or_else(|_| config::AppConfig::load_default());
                let timeout_duration = Duration::from_millis(cfg.orchestrator_tool_timeout_ms);
                let cancellation = self
                    .task_cancellations
                    .entry(task_id.clone())
                    .or_insert_with(tokio_util::sync::CancellationToken::new)
                    .clone();
                self.tasks
                    .entry(task_id.clone())
                    .or_insert_with(|| TaskStateRecord {
                        task_id: task_id.clone(),
                        run_id: durable.run_id.clone(),
                        tool_name: durable.tool_name.clone(),
                        args: durable.args.clone(),
                        query_node_id: durable.query_node_id.clone(),
                        query_id: durable
                            .query_node_id
                            .as_ref()
                            .and_then(|node_id| self.conversation_graph.nodes.get(node_id))
                            .map(|node| node.event_id.clone()),
                        query_graph_version: durable.query_graph_version,
                        playback_generation: self.speech_generation_id,
                        dependent_query_ids: Vec::new(),
                        required_query_ids: Vec::new(),
                        completed_output: None,
                        answer_released: false,
                        status: TaskStatus::Running,
                        attempt: durable.attempt,
                        max_retry: durable.max_attempts,
                        retry_count: durable.retry_count,
                        timeout_duration,
                        error: durable.last_error.clone(),
                    });
                if let Some(task) = self.tasks.get_mut(&task_id) {
                    task.status = TaskStatus::Running;
                    task.attempt = durable.attempt;
                    task.retry_count = durable.retry_count;
                    task.error = durable.last_error.clone();
                }
                self.register_timeout(task_id.clone(), timeout_duration);
                let client = self.orch.tool_client.clone();
                let q_tx = self.orch.queue_tx.clone();
                let inference_semaphore = self.orch.inference_semaphore.clone();
                let task_query_id = self
                    .tasks
                    .get(&task_id)
                    .and_then(|task| task.query_id.clone());
                let task_playback_generation = self
                    .tasks
                    .get(&task_id)
                    .map(|task| task.playback_generation)
                    .unwrap_or(0);
                tracing::info!(session = %self.session_id, task = %task_id.0, attempt = durable.attempt, "executing claimed durable tool task");
                tokio::spawn(async move {
                    let Ok(_permit) = inference_semaphore.acquire_owned().await else {
                        return;
                    };
                    super::executor::execute_tool(
                        client,
                        q_tx,
                        durable.run_id.0,
                        task_id,
                        durable.tool_name,
                        durable.args,
                        durable.attempt,
                        task_query_id,
                        task_playback_generation,
                        cancellation,
                    )
                    .await;
                });
            }

            QueueEvent::GraphNodesFetched {
                session_id,
                user_id,
                language,
                task_id,
                nodes: admin_nodes,
            } => {
                self.clear_timeout(&task_id);

                self.workflow_graph = Some(workflow::SessionWorkflow::create(
                    self.run_id.0.clone(),
                    admin_nodes,
                ));

                let trace_id = format!("trace-{}", chrono::Utc::now().timestamp_millis());
                let task_id_audit = TaskId(format!("task-db-audit-{}", trace_id));
                self.register_timeout(task_id_audit.clone(), Duration::from_secs(5));
                let (tx, rx) = tokio::sync::oneshot::channel();
                let _ = self
                    .orch
                    .db_tx
                    .send(db::actor::DbRequest::PushRedisStream {
                        idempotency_key: task_id_audit.0.clone(),
                        stream_name: "stream:audit".to_string(),
                        session_id: session_id.clone(),
                        trace_id: trace_id.clone(),
                        event_type: "SESSION_STARTED".to_string(),
                        payload_json: json!({ "user_id": user_id, "language": language })
                            .to_string(),
                        reply_to: tx,
                    })
                    .await;
                let q_tx = self.orch.queue_tx.clone();
                tokio::spawn(async move {
                    let (success, error) = match rx.await {
                        Ok(Ok(_)) => (true, None),
                        Ok(Err(e)) => (false, Some(e)),
                        Err(e) => (false, Some(e.to_string())),
                    };
                    let _ = q_tx
                        .send(QueueEvent::DbActionCompleted {
                            task_id: task_id_audit,
                            action: "PushRedisStream".to_string(),
                            success,
                            error,
                        })
                        .await;
                });

                if !session_id.starts_with("phone:") {
                    let _ = self.orch.pubsub.publish(SystemEvent::CallSessionStarted {
                        call_id: session_id,
                        caller_id: user_id,
                        language,
                    });
                }
            }

            QueueEvent::DbActionCompleted {
                task_id,
                action,
                success,
                error,
            } => {
                self.clear_timeout(&task_id);
                if !success {
                    eprintln!(
                        "[SessionActor {}] DbAction '{}' failed for task {}: {:?}",
                        self.session_id, action, task_id.0, error
                    );
                }
            }

            QueueEvent::InterviewerSpeech { active, .. } => {
                self.ai_listen_muted = active;
                if active {
                    if let Some(sts_tx) = &self.sts_tx {
                        let _ = sts_tx.send(crate::models::StsPipelineCmd::Stop).await;
                    }
                }
                if let Some(tx) = &self.agent_tx {
                    let _ = tx
                        .send(AgentCmd::SetAiListenMute {
                            run_id: self.run_id.clone(),
                            muted: active,
                        })
                        .await;
                }
            }

            QueueEvent::BargeInTrigger {
                session_id,
                timestamp_ms,
            } => {
                self.suppress_agent_response_until_next_user_turn = true;
                self.set_llm_speaker_state(true, false);
                self.set_user_speaking(true);
                // Clear transport queues first so already-buffered speech
                // cannot leak through after the caller starts talking.
                if let Some(sts_tx) = &self.sts_tx {
                    if let Err(tokio::sync::mpsc::error::TrySendError::Full(stop)) =
                        sts_tx.try_send(crate::models::StsPipelineCmd::Stop)
                    {
                        let retry_tx = sts_tx.clone();
                        tokio::spawn(async move {
                            let _ = retry_tx.send(stop).await;
                        });
                    }
                }
                self.output_sequencer
                    .cancel_generation(self.speech_turn_id, self.speech_generation_id);
                self.record_speech_interruption(self.speech_generation_id, timestamp_ms)
                    .await;
                if !session_id.starts_with("phone:") {
                    let _ = self.orch.pubsub.publish(SystemEvent::VoiceBargeIn {
                        call_id: session_id,
                        timestamp_ms,
                    });
                }
            }

            QueueEvent::Tool(tool_event) => {
                self.handle_tool_event(tool_event).await;
            }

            QueueEvent::Agent(agent_event) => {
                self.handle_agent_event(agent_event).await;
            }

            QueueEvent::VerifyInterruptionSpeechState {
                session_id,
                generation_id,
                attempt,
                consecutive_silent_checks,
            } => {
                if self.interruption_verification_generation_id != Some(generation_id)
                    || self.pending_interruption_generation_id != Some(generation_id)
                {
                    tracing::debug!(
                        session = %session_id,
                        generation_id,
                        attempt,
                        "discarding stale interruption speech-state check"
                    );
                    return;
                }
                if self.pending_query_id.is_some() {
                    self.interruption_verification_generation_id = None;
                    self.interruption_verification_waiting_generation_id = None;
                    tracing::info!(
                        session = %session_id,
                        generation_id,
                        attempt,
                        "interruption recovery stopped because a user query is pending"
                    );
                    return;
                }

                let consecutive_silent_checks = if self.user_speaking {
                    0
                } else {
                    consecutive_silent_checks.saturating_add(1)
                };
                tracing::info!(
                    session = %session_id,
                    generation_id,
                    attempt,
                    max_checks = INTERRUPTION_SPEECH_STATE_MAX_CHECKS,
                    user_speaking = self.user_speaking,
                    fast_llm_active = self.fast_llm_active,
                    consecutive_silent_checks,
                    "orchestrator checked caller speech state for interruption recovery"
                );

                if consecutive_silent_checks >= 2 {
                    self.interruption_verification_generation_id = None;
                    self.set_user_speaking(false);
                    self.resume_false_positive_interruption(&session_id, generation_id)
                        .await;
                } else if attempt >= INTERRUPTION_SPEECH_STATE_MAX_CHECKS {
                    self.interruption_verification_generation_id = None;
                    if self.user_speaking {
                        self.interruption_verification_waiting_generation_id = Some(generation_id);
                        tracing::warn!(
                            session = %session_id,
                            generation_id,
                            attempt,
                            "caller still appears to be speaking after the verification limit; waiting for a later silence event before retrying"
                        );
                    } else {
                        self.interruption_verification_waiting_generation_id = None;
                        self.set_user_speaking(false);
                        self.resume_false_positive_interruption(&session_id, generation_id)
                            .await;
                    }
                } else {
                    self.schedule_interruption_speech_state_check(
                        generation_id,
                        attempt.saturating_add(1),
                        consecutive_silent_checks,
                    );
                }
            }

            QueueEvent::StsEvent { session_id, event } => {
                match event {
                    StsPipelineEvent::UserSpeechEnded => {
                        self.set_user_speaking(false);
                        tracing::info!(
                            session = %session_id,
                            "caller silence observed; waiting for transcript verification before resuming interrupted output"
                        );
                        if let Some(generation_id) =
                            self.interruption_verification_waiting_generation_id
                        {
                            if self.pending_interruption_generation_id == Some(generation_id)
                                && self.pending_query_id.is_none()
                                && self.interruption_verification_generation_id.is_none()
                            {
                                self.interruption_verification_waiting_generation_id = None;
                                self.interruption_verification_generation_id = Some(generation_id);
                                self.schedule_interruption_speech_state_check(generation_id, 1, 0);
                            }
                        }
                    }
                    StsPipelineEvent::InterruptionFalsePositive => {
                        tracing::info!(
                            session = %session_id,
                            user_speaking = self.user_speaking,
                            fast_llm_active = self.fast_llm_active,
                            main_llm_active = self.main_llm_active,
                            pending_query = self.pending_query_id.is_some(),
                            pending_interruption = self.pending_interruption_generation_id.is_some(),
                            "received false-positive interruption verdict"
                        );
                        if self.pending_query_id.is_some() {
                            tracing::info!(
                                session = %session_id,
                                "ignoring false-positive interruption verdict because a finalized user query is pending"
                            );
                            return;
                        }
                        let Some(generation_id) = self.pending_interruption_generation_id else {
                            tracing::debug!(
                                session = %session_id,
                                "ignoring false-positive interruption verdict without a pending interruption"
                            );
                            return;
                        };
                        if self.interruption_verification_generation_id == Some(generation_id) {
                            tracing::debug!(
                                session = %session_id,
                                generation_id,
                                "false-positive interruption verification is already scheduled"
                            );
                            return;
                        }
                        self.interruption_verification_waiting_generation_id = None;
                        self.interruption_verification_generation_id = Some(generation_id);
                        self.schedule_interruption_speech_state_check(generation_id, 1, 0);
                    }
                    StsPipelineEvent::UserSpeechStarted => {
                        // VAD may open while the main/fast model is processing,
                        // not only while STS audio is already playing. Always
                        // revoke main-agent speech output as soon as validated
                        // user speech starts; generation/tool work continues.
                        // A later query can supersede its output without cancelling it.
                        self.suppress_agent_response_until_next_user_turn = true;
                        self.set_llm_speaker_state(true, false);
                        self.set_user_speaking(true);
                        if let Some(sts_tx) = &self.sts_tx {
                            if let Err(tokio::sync::mpsc::error::TrySendError::Full(stop)) =
                                sts_tx.try_send(crate::models::StsPipelineCmd::Stop)
                            {
                                let retry_tx = sts_tx.clone();
                                tokio::spawn(async move {
                                    let _ = retry_tx.send(stop).await;
                                });
                            }
                        }
                        self.sts_idle
                            .store(true, std::sync::atomic::Ordering::SeqCst);
                        self.output_sequencer
                            .cancel_generation(self.speech_turn_id, self.speech_generation_id);
                        let timestamp_ms = chrono::Utc::now().timestamp_millis().max(0) as u64;
                        self.record_speech_interruption(self.speech_generation_id, timestamp_ms)
                            .await;
                        tracing::info!(session = %session_id, "validated user speech preempted STS; active generation and pending tools continue until the user turn is finalized");
                    }
                    StsPipelineEvent::BargeIn => {
                        self.suppress_agent_response_until_next_user_turn = true;
                        self.set_llm_speaker_state(true, false);
                        self.set_user_speaking(true);
                        // Priority order is intentional: clear STS/TTS before
                        // persisting the interruption in the graph.
                        if let Some(sts_tx) = &self.sts_tx {
                            if let Err(tokio::sync::mpsc::error::TrySendError::Full(stop)) =
                                sts_tx.try_send(crate::models::StsPipelineCmd::Stop)
                            {
                                let retry_tx = sts_tx.clone();
                                tokio::spawn(async move {
                                    let _ = retry_tx.send(stop).await;
                                });
                            }
                        }
                        self.sts_idle
                            .store(true, std::sync::atomic::Ordering::SeqCst);
                        self.output_sequencer
                            .cancel_generation(self.speech_turn_id, self.speech_generation_id);
                        let timestamp_ms = chrono::Utc::now().timestamp_millis().max(0) as u64;
                        self.record_speech_interruption(self.speech_generation_id, timestamp_ms)
                            .await;
                    }
                    StsPipelineEvent::UserTranscriptChunk {
                        text,
                        is_final,
                        noise,
                    } => {
                        use crate::speech_filter::{sanitize_transcript, CleanedInput};

                        // Always write to S3 immediately (Irrelevant = Redis skips).
                        self.publish_conversation_with_relevance(
                            "USER_PROMPT",
                            serde_json::json!({ "text": text.clone(), "role": "user", "final": is_final }).to_string(),
                            ConversationRelevance::Irrelevant,
                        );

                        if is_final {
                            self.set_user_speaking(false);
                            let last_partial_at = self.last_sts_partial_at.take();
                            let trimmed = text.trim();
                            let cleaned = sanitize_transcript(&text);
                            let now = std::time::Instant::now();
                            let is_dup = if let Some((ref prev_text, ref prev_time)) =
                                self.last_user_transcript
                            {
                                prev_text.eq_ignore_ascii_case(trimmed)
                                    && now.duration_since(*prev_time).as_millis() < 2000
                            } else {
                                false
                            };

                            if is_dup {
                                tracing::info!(
                                "[SessionActor {}] Deduplicated identical user transcript within 2s window: '{}'",
                                session_id, trimmed
                            );
                                if !matches!(&cleaned, CleanedInput::Empty) {
                                    self.pending_interruption_generation_id = None;
                                    self.interruption_verification_generation_id = None;
                                    self.interruption_verification_waiting_generation_id = None;
                                    self.deferred_agent_response = None;
                                }
                            } else {
                                self.last_user_transcript = Some((trimmed.to_string(), now));
                                self.pending_user_turn = Some(text.clone());
                                if self.ai_listen_muted {
                                    if !matches!(&cleaned, CleanedInput::Empty) {
                                        self.pending_interruption_generation_id = None;
                                        self.interruption_verification_generation_id = None;
                                        self.interruption_verification_waiting_generation_id = None;
                                        self.deferred_agent_response = None;
                                    }
                                    tracing::warn!(
                                        session = %session_id,
                                        transcript = %trimmed,
                                        "final user transcript discarded because AI listening is muted"
                                    );
                                } else {
                                    match cleaned {
                                        CleanedInput::Empty => {
                                            tracing::info!(
                                                "[SessionActor {}] STS transcript is noise-only — skipping LLM: '{}'",
                                                session_id, text
                                            );
                                            // S3 already written above; skip Redis + LLM.
                                            if self.pending_interruption_generation_id.is_some() {
                                                if let Err(error) = self
                                                    .orch
                                                    .queue_tx
                                                    .send(QueueEvent::StsEvent {
                                                        session_id: session_id.clone(),
                                                        event: StsPipelineEvent::InterruptionFalsePositive,
                                                    })
                                                    .await
                                                {
                                                    tracing::error!(
                                                        session = %session_id,
                                                        %error,
                                                        "could not schedule false-positive interruption recovery"
                                                    );
                                                }
                                            }
                                        }
                                        CleanedInput::Continuation(kind) => {
                                            self.deferred_agent_response = None;
                                            self.pending_interruption_generation_id = None;
                                            self.interruption_verification_generation_id = None;
                                            self.interruption_verification_waiting_generation_id =
                                                None;
                                            self.active_speech_response_node_id = None;
                                            self.active_speech_query_node_id = None;
                                            self.suppress_agent_response_until_next_user_turn =
                                                false;
                                            self.set_llm_speaker_state(true, false);
                                            if let Some(agent_tx) = &self.agent_tx {
                                                if let Err(error) = agent_tx
                                                    .send(crate::models::AgentCmd::ContinueLast {
                                                        run_id: self.run_id.clone(),
                                                        kind,
                                                    })
                                                    .await
                                                {
                                                    tracing::error!(
                                                        session = %session_id,
                                                        %error,
                                                        "continuation transcript could not be dispatched to agent"
                                                    );
                                                }
                                            } else {
                                                tracing::error!(
                                                    session = %session_id,
                                                    "continuation transcript dropped because the agent channel is unavailable"
                                                );
                                            }
                                        }
                                        CleanedInput::Text(clean) => {
                                            tracing::info!(
                                                session = %session_id,
                                                transcript_chars = text.chars().count(),
                                                clean_chars = clean.chars().count(),
                                                "accepted final STS transcript; scheduling agent dispatch"
                                            );
                                            self.suppress_agent_response_until_next_user_turn =
                                                false;
                                            self.deferred_agent_response = None;
                                            self.pending_interruption_generation_id = None;
                                            self.interruption_verification_generation_id = None;
                                            self.interruption_verification_waiting_generation_id =
                                                None;
                                            self.active_speech_response_node_id = None;
                                            self.active_speech_query_node_id = None;
                                            self.set_llm_speaker_state(true, false);
                                            self.publish_conversation_with_relevance(
                                                "USER_PROMPT",
                                                serde_json::json!({ "text": clean, "role": "user", "final": true }).to_string(),
                                                ConversationRelevance::Relevant,
                                            );
                                            if let Some(user_node_id) =
                                                self.conversation_graph.append_user_text(&text)
                                            {
                                                let _ = self
                                                    .conversation_graph
                                                    .append_optimized_intent(
                                                        clean.clone(),
                                                        &user_node_id,
                                                    );
                                                self.publish_graph_state().await;
                                                let query_id = self
                                                    .conversation_graph
                                                    .nodes
                                                    .get(&user_node_id)
                                                    .map(|node| node.event_id.clone())
                                                    .unwrap_or_else(|| {
                                                        uuid::Uuid::new_v4().to_string()
                                                    });
                                                self.pending_query_id = Some(query_id.clone());
                                                self.pending_query_text = Some(text.clone());
                                                self.pending_query_clean = Some(clean);
                                                self.pending_query_noise = noise.clone();
                                                self.pending_query_started_at =
                                                    Some(std::time::Instant::now());
                                                let queue_tx = self.orch.queue_tx.clone();
                                                let session_for_commit = session_id.clone();
                                                let cfg = config::AppConfig::load().unwrap_or_else(
                                                    |_| config::AppConfig::load_default(),
                                                );
                                                let debounce = Duration::from_millis(
                                                    cfg.sts_text_jitter_ms
                                                        .min(cfg.sts_text_max_wait_ms.max(1)),
                                                );
                                                // This is a true debounce, not a fixed delay:
                                                // every partial ASR update resets the quiet
                                                // window. The fast planner receives only this
                                                // final, settled transcript and never a fragment.
                                                let delay = last_partial_at
                                                    .map(|at| debounce.saturating_sub(at.elapsed()))
                                                    .unwrap_or(debounce);
                                                tokio::spawn(async move {
                                                    tokio::time::sleep(delay).await;
                                                    let _ = queue_tx
                                                        .send(QueueEvent::CommitTranscript {
                                                            session_id: session_for_commit,
                                                            query_id,
                                                        })
                                                        .await;
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            // Partials are transport-only evidence. Keep them
                            // out of the agent/tool path and move the debounce
                            // deadline forward until the caller stops speaking.
                            self.last_sts_partial_at = Some(std::time::Instant::now());
                            tracing::debug!(
                                session = %session_id,
                                transcript_chars = text.chars().count(),
                                "received partial STT transcript; reset fast-planner debounce"
                            );
                        }
                    }
                    StsPipelineEvent::AiTranscriptChunk { text } => {
                        self.sts_idle
                            .store(false, std::sync::atomic::Ordering::SeqCst);
                        // The agent response is the canonical assistant transcript and was
                        // already broadcast before STS began speech synthesis. Keep this
                        // callback for transport observability without duplicating history.
                        tracing::debug!(%session_id, %text, "STS assistant transcript received");
                    }
                    StsPipelineEvent::AudioComplete => {
                        // The completed answer no longer owns the transport;
                        // fast planning owns the next turn until main-agent
                        // output is explicitly authorized.
                        self.set_llm_speaker_state(true, false);
                        self.sts_idle
                            .store(true, std::sync::atomic::Ordering::SeqCst);
                        if self.conversation_graph.mark_latest_response_spoken() {
                            self.publish_graph_state().await;
                        }
                        self.active_speech_response_node_id = None;
                        self.active_speech_query_node_id = None;
                        let sts_task_id = TaskId(format!("sts-{}", session_id));
                        if self.clear_timeout(&sts_task_id) {
                            if let Some(task) = self.tasks.get_mut(&sts_task_id) {
                                task.status = TaskStatus::Succeeded;
                            }
                        }
                    }
                    StsPipelineEvent::Error(error) => {
                        self.set_llm_speaker_state(true, false);
                        self.sts_idle
                            .store(true, std::sync::atomic::Ordering::SeqCst);
                        tracing::error!(
                            "Speech pipeline error for session {}: {}",
                            session_id,
                            error
                        );
                        let sts_task_id = TaskId(format!("sts-{}", session_id));
                        self.clear_timeout(&sts_task_id);
                        if let Some(task) = self.tasks.get_mut(&sts_task_id) {
                            task.status = TaskStatus::Failed;
                            task.error = Some(error);
                        }
                    }
                }
            }

            _ => {}
        }
    }
}
