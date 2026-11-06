use super::super::session::SessionActor;
use crate::models::*;
use pubsub::SystemEvent;
use serde_json::json;
use std::time::Duration;

impl SessionActor {
    pub async fn handle_queue_event(&mut self, event: QueueEvent) {
        match event {
            QueueEvent::SessionCancelled { session_id } => {
                self.orch.phone_media.close(&session_id);
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

            QueueEvent::UserAudioChunk {
                session_id,
                pcm_base64,
                ..
            } => {
                let bytes = pcm_base64.into_bytes();
                // Fan out to Tone Analysis and other parallel broadcast listeners.
                let _ = self.audio_broadcast_tx.send((session_id.clone(), bytes));
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
                let bytes = samples
                    .iter()
                    .flat_map(|sample| sample.to_le_bytes())
                    .collect::<Vec<_>>();
                let _ = self.audio_broadcast_tx.send((session_id, bytes));
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
                                self.suppress_agent_response_until_next_user_turn = false;
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
                reply_to,
            } => {
                // The expensive serialization is deliberately demand-only.
                // Normal graph mutation, retries, and timeout events carry
                // IDs/revisions only and never pass graph data through the
                // orchestrator queue.
                let context = self
                    .conversation_graph
                    .relevant_snapshot_json_for_query(&query_id);
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
                if self
                    .conversation_graph
                    .upsert_graph_enrichment_for_query(&query_id, base_graph_version, enrichment)
                    .is_some()
                {
                    self.publish_graph_state().await;
                } else {
                    tracing::debug!(session = %self.session_id, %query_id, base_graph_version, current_graph_version = self.conversation_graph.graph_version, "discarding stale graph enrichment");
                }
            }

            QueueEvent::CommitTranscript { query_id, .. } => {
                if self.pending_query_id.as_deref() != Some(query_id.as_str()) {
                    return;
                }
                // Final transcript is authoritative evidence of a new caller
                // turn. Stop any old STS response before fast planning turn
                // two, but deliberately preserve pending tools so the planner
                // can relate/reuse them through the graph.
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
                    self.cancel_active_generation_work().await;
                    let timestamp_ms = chrono::Utc::now().timestamp_millis().max(0) as u64;
                    if self
                        .conversation_graph
                        .append_assistant_interrupted(self.speech_generation_id, timestamp_ms)
                        .is_some()
                    {
                        self.publish_graph_state().await;
                    }
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
                // Clear transport queues first. Waiting on an agent mailbox or
                // a slow tool cancellation lets already-buffered speech leak
                // through after the caller has started talking.
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
                self.cancel_active_generation_work().await;
                if self
                    .conversation_graph
                    .append_assistant_interrupted(self.speech_generation_id, timestamp_ms)
                    .is_some()
                {
                    self.publish_graph_state().await;
                }
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

            QueueEvent::StsEvent { session_id, event } => {
                match event {
                    StsPipelineEvent::UserSpeechStarted => {
                        // This event is emitted only after the human-speech
                        // VAD gate has opened. Preempt audio immediately so
                        // old STS chunks cannot reach the caller while their
                        // new utterance is still being transcribed/planned.
                        // Do not cancel tools: fast planning decides their
                        // relationship after the final transcript exists.
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
                            self.cancel_active_generation_work().await;
                            tracing::info!(session = %session_id, "validated user speech preempted active STS playback before fast planning");
                        }
                    }
                    StsPipelineEvent::BargeIn => {
                        self.suppress_agent_response_until_next_user_turn = true;
                        // Priority order is intentional: clear STS/TTS before
                        // awaiting cancellation or graph persistence.
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
                        self.cancel_active_generation_work().await;
                        let timestamp_ms = chrono::Utc::now().timestamp_millis().max(0) as u64;
                        if self
                            .conversation_graph
                            .append_assistant_interrupted(self.speech_generation_id, timestamp_ms)
                            .is_some()
                        {
                            self.publish_graph_state().await;
                        }
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
                            let trimmed = text.trim();
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
                            } else {
                                self.last_user_transcript = Some((trimmed.to_string(), now));
                                self.pending_user_turn = Some(text.clone());
                                if self.ai_listen_muted {
                                    println!(
                                    "[SessionActor {}] AI is muted/paused. Discarding user transcript: {}",
                                    session_id, text
                                );
                                } else {
                                    match sanitize_transcript(&text) {
                                        CleanedInput::Empty => {
                                            tracing::info!(
                                                "[SessionActor {}] STS transcript is noise-only — skipping LLM: '{}'",
                                                session_id, text
                                            );
                                            // S3 already written above; skip Redis + LLM.
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
                                            self.suppress_agent_response_until_next_user_turn =
                                                false;
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
                                                let delay_ms = cfg
                                                    .sts_text_jitter_ms
                                                    .min(cfg.sts_text_max_wait_ms.max(1));
                                                tokio::spawn(async move {
                                                    tokio::time::sleep(Duration::from_millis(
                                                        delay_ms,
                                                    ))
                                                    .await;
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
                        self.sts_idle
                            .store(true, std::sync::atomic::Ordering::SeqCst);
                        let _ = self.conversation_graph.mark_latest_response_spoken();
                        let sts_task_id = TaskId(format!("sts-{}", session_id));
                        if self.clear_timeout(&sts_task_id) {
                            if let Some(task) = self.tasks.get_mut(&sts_task_id) {
                                task.status = TaskStatus::Succeeded;
                            }
                        }
                    }
                    StsPipelineEvent::Error(error) => {
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
