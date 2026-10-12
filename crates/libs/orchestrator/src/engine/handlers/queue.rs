use super::super::session::SessionActor;
use crate::models::*;
use pubsub::SystemEvent;
use serde_json::json;
use std::collections::HashMap;
use std::time::Duration;

impl SessionActor {
    pub async fn handle_queue_event(&mut self, event: QueueEvent) {
        match event {
            QueueEvent::SessionCancelled { session_id } => {
                println!("[SessionActor {}] Cancel event received.", session_id);
                if let Some(graph) = &mut self.workflow_graph {
                    graph.status = "Cancelled".to_string();
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

                        let audio_url =
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
                                text_url: String::new(),
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

            QueueEvent::SttTranscript {
                session_id,
                text,
                is_final,
                ..
            } => {
                if is_final {
                    if self.ai_listen_muted {
                        println!(
                            "[SessionActor {}] AI is muted. Discarding STT transcript: {}",
                            session_id, text
                        );
                    } else if let Some(agent_tx) = &self.agent_tx {
                        let _ = agent_tx
                            .send(crate::models::AgentCmd::UserInput {
                                run_id: self.run_id.clone(),
                                user_id: session_id.clone(),
                                role: crate::models::SpeakerRole::Candidate,
                                text: text.clone(),
                            })
                            .await;
                    }
                }
            }

            QueueEvent::GraphNodesFetched {
                session_id,
                user_id,
                language,
                task_id,
                nodes: admin_nodes,
            } => {
                self.clear_timeout(&task_id);

                let mut nodes = HashMap::new();
                let mut first_node_id = None;

                for admin_node in admin_nodes {
                    if first_node_id.is_none() {
                        first_node_id = Some(admin_node.id.clone());
                    }
                    nodes.insert(
                        admin_node.id.clone(),
                        WorkflowNode {
                            id: admin_node.id,
                            step_type: admin_node.step_type,
                            instruction: admin_node.instruction,
                            status: "Pending".to_string(),
                        },
                    );
                }

                if nodes.is_empty() {
                    nodes.insert(
                        "step-1".to_string(),
                        WorkflowNode {
                            id: "step-1".to_string(),
                            step_type: "UserInputVerifier".to_string(),
                            instruction: "Scan prompt & verify user KYC status".to_string(),
                            status: "Completed".to_string(),
                        },
                    );
                    nodes.insert(
                        "step-2".to_string(),
                        WorkflowNode {
                            id: "step-2".to_string(),
                            step_type: "RagFetchAndAgentAnalysis".to_string(),
                            instruction: "Fetch RAG listings & execute heavy Agent analysis"
                                .to_string(),
                            status: "Executing".to_string(),
                        },
                    );
                }

                let active_node = first_node_id.unwrap_or_else(|| "step-2".to_string());
                self.workflow_graph = Some(WorkflowGraph {
                    run_id: self.run_id.clone(),
                    active_node_id: active_node,
                    nodes,
                    status: "Active".to_string(),
                });

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

                let _ = self.orch.pubsub.publish(SystemEvent::CallSessionStarted {
                    call_id: session_id,
                    caller_id: user_id,
                    language,
                });
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
                let _ = self.orch.pubsub.publish(SystemEvent::VoiceBargeIn {
                    call_id: session_id,
                    timestamp_ms,
                });
                if let Some(sts_tx) = &self.sts_tx {
                    let _ = sts_tx.send(crate::models::StsPipelineCmd::Stop).await;
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
                    StsPipelineEvent::BargeIn => {
                        if let Some(sts_tx) = &self.sts_tx {
                            let _ = sts_tx.send(crate::models::StsPipelineCmd::Stop).await;
                        }
                    }
                    StsPipelineEvent::UserTranscriptChunk { text, is_final } => {
                        if let Some(s3_tx) = &self.s3_dump_tx {
                            if s3_tx
                                .try_send(S3DumpData::Text {
                                    session_id: session_id.clone(),
                                    payload: format!(
                                        "user_transcript: {} (final: {})",
                                        text, is_final
                                    ),
                                })
                                .is_err()
                            {
                                tracing::trace!("Session {} recording text queue full; dropping transcript copy", session_id);
                            }
                        }

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
                                if self.ai_listen_muted {
                                    println!(
                                    "[SessionActor {}] AI is muted/paused. Discarding user transcript: {}",
                                    session_id, text
                                );
                                } else if let Some(agent_tx) = &self.agent_tx {
                                    let _ = agent_tx
                                        .send(crate::models::AgentCmd::UserInput {
                                            run_id: self.run_id.clone(),
                                            user_id: session_id.clone(),
                                            role: crate::models::SpeakerRole::Candidate,
                                            text: text.clone(),
                                        })
                                        .await;
                                }
                            }
                        }
                    }
                    StsPipelineEvent::AiTranscriptChunk { text } => {
                        if let Some(s3_tx) = &self.s3_dump_tx {
                            if s3_tx
                                .try_send(S3DumpData::Text {
                                    session_id: session_id.clone(),
                                    payload: format!("ai_transcript: {}", text),
                                })
                                .is_err()
                            {
                                tracing::trace!("Session {} recording text queue full; dropping transcript copy", session_id);
                            }
                        }
                    }
                    StsPipelineEvent::AudioComplete => {
                        let sts_task_id = TaskId(format!("sts-{}", session_id));
                        if self.clear_timeout(&sts_task_id) {
                            if let Some(task) = self.tasks.get_mut(&sts_task_id) {
                                task.status = TaskStatus::Succeeded;
                            }
                        }
                    }
                    StsPipelineEvent::Error(error) => {
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
