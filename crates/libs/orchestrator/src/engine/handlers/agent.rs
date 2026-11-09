use super::super::session::SessionActor;
use super::executor::execute_tool;
use crate::models::*;
use futures_util::FutureExt;
use pubsub::SystemEvent;
use serde_json::json;
use std::panic::AssertUnwindSafe;
use std::time::Duration;

impl SessionActor {
    pub async fn handle_agent_event(&mut self, event: AgentEvent) {
        match event {
            AgentEvent::ToolRequested {
                run_id,
                task_id,
                query_id,
                playback_generation,
                depends_on_query_ids,
                tool_name,
                wait_message,
                args,
            } => {
                let cfg =
                    config::AppConfig::load().unwrap_or_else(|_| config::AppConfig::load_default());
                // The fast model proposes dependency IDs; the session actor
                // authorizes only live, prior task branches. This rejects
                // fabricated IDs, self-dependencies, duplicates, and the one
                // backwards edge that could form a cycle.
                let mut validated_dependencies = depends_on_query_ids;
                validated_dependencies.sort();
                validated_dependencies.dedup();
                validated_dependencies.retain(|dependency_id| {
                    query_id.as_deref() != Some(dependency_id.as_str())
                        && self.tasks.values().any(|task| {
                            task.query_id.as_deref() == Some(dependency_id.as_str())
                                && matches!(task.status, TaskStatus::Running | TaskStatus::Retrying)
                                && !query_id.as_ref().is_some_and(|current_id| {
                                    task.required_query_ids.contains(current_id)
                                })
                        })
                });
                let is_search = tool_name == "search_properties"
                    || tool_name.to_ascii_lowercase().contains("search")
                    || tool_name.to_ascii_lowercase().contains("rag");
                let sts_idle = self.sts_idle.load(std::sync::atomic::Ordering::SeqCst);
                let wait_message = wait_message
                    .filter(|message| !message.trim().is_empty())
                    .unwrap_or_else(|| cfg.tool_wait_tts_prompt.clone());
                if is_search
                    && sts_idle
                    && !wait_message.trim().is_empty()
                    && !self.suppress_agent_response_until_next_user_turn
                {
                    self.set_llm_speaker_state(true, false);
                    self.sts_idle
                        .store(false, std::sync::atomic::Ordering::SeqCst);
                    if let Some(sts_tx) = &self.sts_tx {
                        match AssertUnwindSafe(sts_tx.send(
                            crate::models::StsPipelineCmd::InjectFastContext { text: wait_message },
                        ))
                        .catch_unwind()
                        .await
                        {
                            Ok(Ok(())) => {}
                            Ok(Err(error)) => tracing::warn!(%error, "Tool wait prompt could not be queued for TTS"),
                            Err(_) => tracing::error!("Tool wait prompt TTS delivery panicked; tool execution will continue"),
                        }
                    }
                }

                let task_record = TaskStateRecord {
                    task_id: task_id.clone(),
                    run_id: run_id.clone(),
                    tool_name: tool_name.clone(),
                    args: args.clone(),
                    query_node_id: query_id.as_ref().and_then(|query_id| {
                        self.conversation_graph
                            .nodes
                            .iter()
                            .find_map(|(node_id, node)| {
                                (node.event_id == *query_id).then_some(node_id.clone())
                            })
                    }),
                    query_id,
                    query_graph_version: self.conversation_graph.graph_version,
                    playback_generation,
                    dependent_query_ids: Vec::new(),
                    required_query_ids: validated_dependencies,
                    completed_output: None,
                    answer_released: false,
                    status: TaskStatus::Running,
                    attempt: 1,
                    max_retry: cfg.orchestrator_tool_max_retries,
                    retry_count: 0,
                    timeout_duration: Duration::from_millis(cfg.orchestrator_tool_timeout_ms),
                    error: None,
                };

                self.tasks.insert(task_id.clone(), task_record);
                if let Some(parent_id) = self
                    .tasks
                    .get(&task_id)
                    .and_then(|task| task.query_node_id.clone())
                {
                    let resolved_query = args
                        .get("query")
                        .and_then(|value| value.as_str())
                        .unwrap_or(&tool_name);
                    if self
                        .conversation_graph
                        .append_optimized_intent(
                            format!("Resolved tool intent: {resolved_query}"),
                            &parent_id,
                        )
                        .is_some()
                    {
                        self.publish_graph_state().await;
                    }
                }
                if let Some(task) = self.tasks.get(&task_id) {
                    self.persist_durable_task(
                        task,
                        crate::durable_scheduler::DurableTaskStatus::Claimed,
                        chrono::Utc::now().timestamp_millis(),
                    );
                }
                let cancellation = tokio_util::sync::CancellationToken::new();
                self.task_cancellations
                    .insert(task_id.clone(), cancellation.clone());
                self.register_timeout(
                    task_id.clone(),
                    Duration::from_millis(cfg.orchestrator_tool_timeout_ms),
                );

                let client = self.orch.tool_client.clone();
                let q_tx = self.orch.queue_tx.clone();
                let r_id = run_id.0.clone();
                let t_id = task_id.clone();
                let t_name = tool_name.clone();
                let task_query_id = self
                    .tasks
                    .get(&task_id)
                    .and_then(|task| task.query_id.clone());
                let task_playback_generation = self
                    .tasks
                    .get(&task_id)
                    .map(|task| task.playback_generation)
                    .unwrap_or(0);
                let inference_semaphore = self.orch.inference_semaphore.clone();

                tokio::spawn(async move {
                    let Ok(_permit) = inference_semaphore.acquire_owned().await else {
                        return;
                    };
                    let result = AssertUnwindSafe(execute_tool(
                        client,
                        q_tx,
                        r_id.clone(),
                        t_id.clone(),
                        t_name.clone(),
                        args,
                        1,
                        task_query_id,
                        task_playback_generation,
                        cancellation,
                    ))
                    .catch_unwind()
                    .await;
                    if result.is_err() {
                        tracing::error!(
                            session_run = %r_id,
                            task = %t_id.0,
                            tool = %t_name,
                            "tool execution task panicked; result remains graph-retained"
                        );
                    }
                });
            }

            AgentEvent::Finished {
                run_id,
                query_id,
                playback_generation,
                result,
                relevance,
            } => {
                if playback_generation != self.speech_generation_id {
                    tracing::info!(session = %self.session_id, expected = self.speech_generation_id, received = playback_generation, "dropping agent response from stale playback generation");
                    return;
                }
                if let Some(query_id) = query_id.as_deref() {
                    let is_current = self
                        .conversation_graph
                        .latest_user_node
                        .as_ref()
                        .and_then(|node_id| self.conversation_graph.nodes.get(node_id))
                        .is_some_and(|node| node.event_id == query_id);
                    if !is_current {
                        tracing::info!(session = %self.session_id, %query_id, "dropping agent response for superseded user query");
                        return;
                    }
                }
                let graph_reconstructed = self.pending_graph_response_version.is_some();
                if let Some(expected_version) = self.pending_graph_response_version.take() {
                    if expected_version != self.conversation_graph.graph_version {
                        tracing::info!(session = %self.session_id, expected = expected_version, current = self.conversation_graph.graph_version, "dropping superseded graph response before speech");
                        self.publish_graph_state().await;
                        return;
                    }
                }
                if self.suppress_agent_response_until_next_user_turn {
                    self.set_llm_speaker_state(true, false);
                    let interruption_generation = *self
                        .pending_interruption_generation_id
                        .get_or_insert(playback_generation);
                    let response_id = self.conversation_graph.append_assistant_response(
                        result.clone(),
                        Some(format!("response:{}:{}", run_id.0, playback_generation)),
                    );
                    let query_node_id = self.conversation_graph.latest_user_node.clone();
                    let interruption_id = self
                        .conversation_graph
                        .append_assistant_interrupted_for_nodes(
                            interruption_generation,
                            chrono::Utc::now().timestamp_millis().max(0) as u64,
                            response_id.as_deref(),
                            query_node_id.as_deref(),
                        );
                    if response_id.is_some() || interruption_id.is_some() {
                        self.publish_graph_state().await;
                    }
                    let deferred = response_id.is_some();
                    if let Some(response_node_id) = response_id {
                        self.active_speech_response_node_id = Some(response_node_id.clone());
                        self.active_speech_query_node_id = query_node_id.clone();
                        self.deferred_agent_response = Some(DeferredAgentResponse {
                            run_id,
                            query_id: query_id.clone(),
                            query_node_id,
                            response_node_id,
                            playback_generation,
                            result: result.clone(),
                            relevance,
                        });
                    }
                    if let Some(graph) = &mut self.workflow_graph {
                        graph.complete();
                    }
                    if relevance.is_relevant() {
                        self.pending_user_turn = None;
                        if graph_reconstructed {
                            self.publish_conversation_with_relevance(
                                "LLM_RESPONSE",
                                serde_json::json!({
                                    "response_text": result,
                                    "source": "graph_reconstruction"
                                })
                                .to_string(),
                                ConversationRelevance::Relevant,
                            );
                        }
                    } else {
                        self.pending_user_turn = None;
                        tracing::info!(
                            session = %self.session_id,
                            "Irrelevant interrupted exchange suppressed from Redis AI history."
                        );
                    }
                    tracing::info!(
                        session = %self.session_id,
                        ?query_id,
                        playback_generation,
                        deferred,
                        "retained interrupted agent response and deferred STS delivery until caller silence"
                    );
                    return;
                }
                // This is the handoff event from fast planning to caller-facing
                // main-model output. Stale/barred completions returned above.
                self.set_llm_speaker_state(false, true);
                if !self.main_llm_may_emit() {
                    tracing::info!(session = %self.session_id, ?query_id, "main-agent completion blocked by current speaker ownership or suppression");
                    return;
                }
                if !graph_reconstructed {
                    let pending = self.conversation_graph.pending_llm_node_ids();
                    self.conversation_graph.mark_llm_seen(&pending);
                }
                let query_node_id = self.conversation_graph.latest_user_node.clone();
                let response_node_id = self.conversation_graph.append_assistant_response(
                    result.clone(),
                    Some(format!("response:{}:{}", run_id.0, playback_generation)),
                );
                if let Some(response_node_id) = response_node_id {
                    self.active_speech_response_node_id = Some(response_node_id);
                    self.active_speech_query_node_id = query_node_id;
                    self.publish_graph_state().await;
                }
                if let Some(graph) = &mut self.workflow_graph {
                    graph.complete();
                }

                // The cleaned user turn is appended to Redis immediately after
                // final STT. Graph reconstruction bypasses the agent actor, so
                // this is the one place that must publish its assistant reply.
                if relevance.is_relevant() {
                    self.pending_user_turn = None;
                    if graph_reconstructed {
                        self.publish_conversation_with_relevance(
                            "LLM_RESPONSE",
                            serde_json::json!({ "response_text": result, "source": "graph_reconstruction" })
                                .to_string(),
                            ConversationRelevance::Relevant,
                        );
                    }
                } else {
                    // The raw turn remains in S3; suppress a domain fallback
                    // response from the Redis context window.
                    self.pending_user_turn = None;
                    tracing::info!(
                        session = %self.session_id,
                        "Irrelevant exchange suppressed from Redis AI history (domain fallback)."
                    );
                }

                self.emit_ordered_speech(result.clone()).await;

                let task_id_cache = TaskId(format!("task-db-cache-{}", run_id.0));
                self.dispatch_db_action(
                    task_id_cache,
                    PendingDbAction::SetJsonWithTtl {
                        key: format!("cache:llm:{}", run_id.call_id()),
                        payload_json: serde_json::json!({ "response": result }).to_string(),
                        ttl_seconds: 1500,
                    },
                    false,
                )
                .await;
            }

            AgentEvent::Failed { run_id, error } => {
                if self.suppress_agent_response_until_next_user_turn {
                    return;
                }
                self.set_llm_speaker_state(true, false);
                if let Some(graph) = &mut self.workflow_graph {
                    graph.fail();
                }

                let fallback_text = "I apologize, but I had trouble processing that request. Please let me know how I can assist you with luxury apartments, viewing slots, or tenancy contracts in the UAE.".to_string();

                // A phone session has a provider media transport. Do not leak
                // its fallback speech or call data to browser subscribers.
                if !run_id.call_id().starts_with("phone:") {
                    let _ = self.orch.pubsub.publish(SystemEvent::AuditEvent {
                        call_id: Some(run_id.call_id().to_string()),
                        event_type: "SYSTEM_ERROR".to_string(),
                        payload: json!({ "error": error, "action": "decision_maker_fallback" }),
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    });

                    let _ = self.orch.pubsub.publish(SystemEvent::VoiceTtsStream {
                        call_id: run_id.call_id().to_string(),
                        audio_chunk_base64: fallback_text.clone(),
                        index: 0,
                        is_end: true,
                    });
                }

                if let Some(sts_tx) = &self.sts_tx {
                    let _ = sts_tx
                        .send(crate::models::StsPipelineCmd::InjectFastContext {
                            text: fallback_text,
                        })
                        .await;
                }
            }

            AgentEvent::NeedsUserInput { run_id, question } => {
                if self.suppress_agent_response_until_next_user_turn {
                    return;
                }
                self.set_llm_speaker_state(true, false);
                if self
                    .conversation_graph
                    .append_assistant_response(
                        question.clone(),
                        Some(format!(
                            "clarify:{}:{}",
                            run_id.0, self.speech_generation_id
                        )),
                    )
                    .is_some()
                {
                    self.publish_graph_state().await;
                }
                if let Some(sts_tx) = &self.sts_tx {
                    let _ = sts_tx
                        .send(crate::models::StsPipelineCmd::InjectFastContext { text: question })
                        .await;
                }
            }

            AgentEvent::WaitForRelatedTool {
                run_id: _,
                query_id,
                depends_on_query_id,
                wait_message,
            } => {
                let mut permitted = false;
                for task in self.tasks.values_mut() {
                    if task.query_id.as_deref() == Some(depends_on_query_id.as_str())
                        && matches!(task.status, TaskStatus::Running | TaskStatus::Retrying)
                    {
                        if !task.dependent_query_ids.contains(&query_id) {
                            task.dependent_query_ids.push(query_id.clone());
                        }
                        permitted = true;
                    }
                }
                if permitted {
                    if self.suppress_agent_response_until_next_user_turn {
                        return;
                    }
                    self.set_llm_speaker_state(true, false);
                    if let (Some(message), Some(sts_tx)) = (wait_message, &self.sts_tx) {
                        let _ = sts_tx
                            .send(crate::models::StsPipelineCmd::InjectFastContext {
                                text: message,
                            })
                            .await;
                    }
                    tracing::info!(session = %self.session_id, %query_id, %depends_on_query_id, "granted related-query answer permit to pending tool");
                } else {
                    tracing::info!(session = %self.session_id, %query_id, %depends_on_query_id, "related tool is no longer pending; planner will re-evaluate on next turn");
                }
            }
        }
    }
}
