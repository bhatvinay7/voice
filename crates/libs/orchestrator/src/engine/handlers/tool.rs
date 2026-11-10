use super::super::session::SessionActor;
use crate::models::*;
use pubsub::SystemEvent;
use serde_json::json;

impl SessionActor {
    pub async fn handle_tool_event(&mut self, event: ToolEvent) {
        match event {
            ToolEvent::Heartbeat {
                task_id,
                attempt: _,
                progress_pct,
                query_id,
                playback_generation,
            } => {
                if self.tasks.get(&task_id).is_some_and(|task| {
                    task.query_id != query_id || task.playback_generation != playback_generation
                }) {
                    tracing::debug!(task = %task_id.0, "discarding tool heartbeat with stale query/playback identity");
                    return;
                }
                let mut agent_notification = None;
                let timeout = if let Some(task) = self.tasks.get(&task_id) {
                    agent_notification = Some((task.run_id.clone(), task_id.clone(), progress_pct));
                    Some(task.timeout_duration)
                } else {
                    None
                };

                if let Some((run_id, t_id, pct)) = agent_notification {
                    if let Some(tx) = &self.agent_tx {
                        let _ = tx
                            .send(AgentCmd::ToolHeartbeat {
                                run_id,
                                task_id: t_id,
                                progress_pct: pct,
                            })
                            .await;
                    }
                }

                if let Some(dur) = timeout {
                    self.register_timeout(task_id.clone(), dur);
                    if let Err(error) = self.orch.durable_scheduler.renew_lease(
                        &task_id,
                        chrono::Utc::now().timestamp_millis(),
                        self.orch.durable_task_lease_ms,
                    ) {
                        tracing::error!(task = %task_id.0, %error, "could not renew durable task lease");
                    }
                }
            }

            ToolEvent::Done {
                task_id,
                attempt,
                output,
                query_id,
                playback_generation,
            } => {
                if attempt != 0
                    && self.tasks.get(&task_id).is_some_and(|task| {
                        task.query_id != query_id || task.playback_generation != playback_generation
                    })
                {
                    tracing::debug!(task = %task_id.0, "discarding tool completion with stale query/playback identity");
                    return;
                }
                if self
                    .tasks
                    .get(&task_id)
                    .is_some_and(|task| attempt != 0 && task.attempt != attempt)
                {
                    tracing::debug!(task = %task_id.0, attempt, "discarding stale tool completion");
                    return;
                }
                if matches!(
                    self.tasks.get(&task_id).map(|task| &task.status),
                    Some(TaskStatus::Cancelled)
                ) {
                    tracing::debug!(task = %task_id.0, "discarding completion from cancelled tool");
                    return;
                }
                self.clear_timeout(&task_id);
                self.task_cancellations.remove(&task_id);
                if let Err(error) = self.orch.durable_scheduler.terminal(
                    &task_id,
                    crate::durable_scheduler::DurableTaskStatus::Succeeded,
                    None,
                ) {
                    tracing::error!(task = %task_id.0, %error, "could not finalize durable tool task");
                }

                let (run_id_opt, tool_name_opt, query_node_id, query_graph_version) =
                    if let Some(task) = self.tasks.get_mut(&task_id) {
                        task.status = TaskStatus::Succeeded;
                        task.completed_output = Some(output.clone());
                        (
                            Some(task.run_id.clone()),
                            Some(task.tool_name.clone()),
                            task.query_node_id.clone(),
                            task.query_graph_version,
                        )
                    } else {
                        (None, None, None, 0)
                    };

                if let (Some(run_id), Some(tool_name)) = (run_id_opt, tool_name_opt) {
                    let parent_ids = query_node_id.clone().into_iter().collect::<Vec<_>>();
                    let query_turn_id = query_node_id
                        .as_ref()
                        .and_then(|node_id| self.conversation_graph.nodes.get(node_id))
                        .map(|node| node.turn_id)
                        .unwrap_or(self.conversation_graph.next_turn_id);
                    let context_node = self.conversation_graph.upsert_tool_context_for_turn(
                        output.to_string(),
                        parent_ids,
                        Some(task_id.0.clone()),
                        query_turn_id,
                    );
                    self.publish_graph_state().await;
                    tracing::debug!(session = %run_id.call_id(), tool = %tool_name, ?context_node, query_graph_version, "tool context merged into its originating graph branch");
                    let trace_id = format!("trace-{}", chrono::Utc::now().timestamp_millis());
                    let task_id_audit = TaskId(format!("task-db-audit-{}", trace_id));
                    self.dispatch_db_action(
                        task_id_audit,
                        PendingDbAction::PushRedisStream {
                            stream_name: "stream:audit".to_string(),
                            session_id: run_id.call_id().to_string(),
                            trace_id: trace_id.clone(),
                            event_type: "TOOL_SUCCESS".to_string(),
                            payload_json: json!({ "task_id": task_id.0, "output": output })
                                .to_string(),
                        },
                        false,
                    )
                    .await;

                    if tool_name.contains("search") || tool_name.contains("rag") {
                        if let Some(suggestions) = output.get("listings") {
                            let cache_key = format!("session:{}:suggestions", run_id.call_id());
                            let payload = json!({
                                "session_id": run_id.call_id(),
                                "timestamp": chrono::Utc::now().to_rfc3339(),
                                "suggestions": suggestions
                            });

                            let task_id_ttl = TaskId(format!("task-db-ttl-{}", run_id.0));
                            self.dispatch_db_action(
                                task_id_ttl,
                                PendingDbAction::SetJsonWithTtl {
                                    key: cache_key.clone(),
                                    payload_json: payload.to_string(),
                                    ttl_seconds: 3600,
                                },
                                false,
                            )
                            .await;

                            if !run_id.call_id().starts_with("phone:") {
                                let _ = self.orch.pubsub.publish(SystemEvent::AuditEvent {
                                    call_id: Some(run_id.call_id().to_string()),
                                    event_type: "DEAL_SUGGESTIONS_GENERATED".to_string(),
                                    payload,
                                    timestamp: chrono::Utc::now().to_rfc3339(),
                                });
                            }
                        }
                    }

                    // The graph is the audit trail for every completed tool.
                    // Main-agent speech is released only after the fast
                    // planner's complete dependency set is terminal.
                    let latest_query_node_id = self.conversation_graph.latest_user_node.clone();
                    let latest_query_id = latest_query_node_id
                        .as_ref()
                        .and_then(|node_id| self.conversation_graph.nodes.get(node_id))
                        .map(|node| node.event_id.clone());
                    let release = if self.suppress_agent_response_until_next_user_turn {
                        tracing::info!(
                            session = %self.session_id,
                            task = %task_id.0,
                            "tool result retained in graph; caller response suppressed until the next accepted user turn"
                        );
                        None
                    } else {
                        latest_query_id.as_ref().and_then(|latest_query_id| {
                            let terminal_queries = self
                                .tasks
                                .values()
                                .filter(|task| {
                                    matches!(
                                        task.status,
                                        TaskStatus::Succeeded
                                            | TaskStatus::Failed
                                            | TaskStatus::TimedOut
                                            | TaskStatus::Cancelled
                                    )
                                })
                                .filter_map(|task| task.query_id.clone())
                                .collect::<std::collections::BTreeSet<_>>();
                            self.tasks.values_mut().find_map(|task| {
                                let targets_latest = task.query_id.as_deref()
                                    == Some(latest_query_id)
                                    || task.dependent_query_ids.contains(latest_query_id);
                                let gate_open = task
                                    .required_query_ids
                                    .iter()
                                    .all(|dependency| terminal_queries.contains(dependency));
                                if targets_latest && gate_open && !task.answer_released {
                                    let output = task.completed_output.clone()?;
                                    task.answer_released = true;
                                    Some((
                                        task.task_id.clone(),
                                        task.run_id.clone(),
                                        task.query_node_id.clone(),
                                        output,
                                        task.query_id.as_deref() != Some(latest_query_id),
                                    ))
                                } else {
                                    None
                                }
                            })
                        })
                    };
                    if let Some((
                        release_task_id,
                        release_run_id,
                        release_node_id,
                        release_output,
                        permitted_for_latest,
                    )) = release
                    {
                        let target_node_id = if permitted_for_latest {
                            latest_query_node_id.as_ref()
                        } else {
                            release_node_id.as_ref()
                        };
                        let user_question = target_node_id
                            .and_then(|node_id| self.conversation_graph.nodes.get(node_id))
                            .map(|node| node.content.clone());
                        let query_id = target_node_id
                            .and_then(|node_id| self.conversation_graph.nodes.get(node_id))
                            .map(|node| node.event_id.clone());
                        let graph_context = user_question.as_ref().and_then(|_| {
                            target_node_id
                                .and_then(|node_id| self.conversation_graph.nodes.get(node_id))
                                .and_then(|node| {
                                    self.read_graph_reference_context(&node.event_id, &[], true)
                                })
                        });
                        let agent_tx = self.agent_tx.clone();
                        if let Some(tx) = agent_tx {
                            self.set_llm_speaker_state(false, true);
                            let _ = tx
                                .send(AgentCmd::ToolResult {
                                    run_id: release_run_id,
                                    task_id: release_task_id,
                                    query_id,
                                    outcome: Ok(release_output),
                                    user_question,
                                    graph_context,
                                })
                                .await;
                        }
                    } else {
                        tracing::info!(
                            session = %self.session_id,
                            task = %task_id.0,
                            ?query_node_id,
                            latest_query = ?self.conversation_graph.latest_user_node,
                            fast_llm_active = self.fast_llm_active,
                            main_llm_active = self.main_llm_active,
                            gate_reason = "task_is_not_the_latest_query_and_has_no_release_dependency",
                            "tool result retained in graph; main-agent answer gate remains closed"
                        );
                    }
                }
            }

            ToolEvent::Failed {
                task_id,
                error,
                retryable,
                attempt,
                query_id,
                playback_generation,
            } => {
                if attempt != 0
                    && self.tasks.get(&task_id).is_some_and(|task| {
                        task.query_id != query_id || task.playback_generation != playback_generation
                    })
                {
                    tracing::debug!(task = %task_id.0, "discarding tool failure with stale query/playback identity");
                    return;
                }
                if self
                    .tasks
                    .get(&task_id)
                    .is_some_and(|task| attempt != 0 && task.attempt != attempt)
                {
                    tracing::debug!(task = %task_id.0, attempt, "discarding stale tool failure");
                    return;
                }
                if matches!(
                    self.tasks.get(&task_id).map(|task| &task.status),
                    Some(TaskStatus::Cancelled)
                ) {
                    tracing::debug!(task = %task_id.0, "discarding failure from cancelled tool");
                    return;
                }
                self.clear_timeout(&task_id);
                // Stop the previous RPC before another attempt is scheduled.
                // A late completion is also rejected by attempt number.
                if let Some(cancellation) = self.task_cancellations.remove(&task_id) {
                    cancellation.cancel();
                }

                let retry_info = if let Some(task) = self.tasks.get_mut(&task_id) {
                    task.retry_count += 1;
                    if retryable && task.retry_count < task.max_retry {
                        task.attempt += 1;
                        task.status = TaskStatus::Retrying;

                        Some((
                            task.timeout_duration,
                            task.run_id.0.clone(),
                            task.tool_name.clone(),
                            task.args.clone(),
                            task.attempt,
                        ))
                    } else {
                        task.status = TaskStatus::Failed;
                        task.error = Some(error.clone());
                        None
                    }
                } else {
                    None
                };

                if let Some((timeout_dur, run_id_str, tool_name, args, attempt)) = retry_info {
                    let cfg = config::AppConfig::load()
                        .unwrap_or_else(|_| config::AppConfig::load_default());
                    let retry_at = chrono::Utc::now().timestamp_millis()
                        + (1_i64 << attempt.min(5)) * cfg.orchestrator_retry_base_ms as i64;
                    if let Some(task) = self.tasks.get(&task_id) {
                        self.persist_durable_task(
                            task,
                            crate::durable_scheduler::DurableTaskStatus::Pending,
                            retry_at,
                        );
                    }
                    if let Some(agent_tx) = &self.agent_tx {
                        let _ = agent_tx
                            .send(AgentCmd::ToolRetrying {
                                run_id: RunId(run_id_str.clone()),
                                task_id: task_id.clone(),
                                attempt,
                            })
                            .await;
                    }
                    let call_id = run_id_str.strip_prefix("run-").unwrap_or(&run_id_str);
                    if !call_id.starts_with("phone:") {
                        let _ = self.orch.pubsub.publish(SystemEvent::AgentToolExecute {
                            call_id: call_id.to_string(),
                            tool_name: tool_name.clone(),
                            args: args.clone(),
                            idempotency_key: task_id.0.clone(),
                        });
                    }

                    tracing::info!(task = %task_id.0, attempt, retry_at, timeout_ms = timeout_dur.as_millis(), tool = %tool_name, args = %args, "durable tool retry scheduled");
                } else if let Some(run_id) = self.tasks.get(&task_id).map(|t| t.run_id.clone()) {
                    self.task_cancellations.remove(&task_id);
                    if let Err(scheduler_error) = self.orch.durable_scheduler.terminal(
                        &task_id,
                        crate::durable_scheduler::DurableTaskStatus::Failed,
                        Some(error.clone()),
                    ) {
                        tracing::error!(task = %task_id.0, %scheduler_error, "could not finalize failed durable tool task");
                    }
                    let trace_id = format!("trace-{}", chrono::Utc::now().timestamp_millis());
                    let task_id_logs = TaskId(format!("task-db-logs-{}", trace_id));
                    self.dispatch_db_action(
                        task_id_logs,
                        PendingDbAction::PushRedisStream {
                            stream_name: "stream:logs".to_string(),
                            session_id: run_id.call_id().to_string(),
                            trace_id: trace_id.clone(),
                            event_type: "TOOL_FAILED".to_string(),
                            payload_json: json!({ "task_id": task_id.0, "error": error })
                                .to_string(),
                        },
                        false,
                    )
                    .await;

                    // A terminal prerequisite failure is graph context, not
                    // an endless wait. Record it so a related successful tool
                    // can still produce an honest partial answer.
                    if let Some(failed_task) = self.tasks.get(&task_id) {
                        if let Some(node_id) = failed_task.query_node_id.clone() {
                            let turn_id = self
                                .conversation_graph
                                .nodes
                                .get(&node_id)
                                .map(|node| node.turn_id)
                                .unwrap_or(self.conversation_graph.next_turn_id);
                            self.conversation_graph.upsert_tool_context_for_turn(
                                format!("Tool {} failed: {}", failed_task.tool_name, error),
                                vec![node_id],
                                Some(format!("{}:terminal-failure", task_id.0)),
                                turn_id,
                            );
                            self.publish_graph_state().await;
                        }
                    }

                    let latest_node = self.conversation_graph.latest_user_node.clone();
                    let latest_query_id = latest_node
                        .as_ref()
                        .and_then(|node_id| self.conversation_graph.nodes.get(node_id))
                        .map(|node| node.event_id.clone());
                    let release = latest_query_id.as_ref().and_then(|latest_query_id| {
                        let terminal_queries = self
                            .tasks
                            .values()
                            .filter(|task| {
                                matches!(
                                    task.status,
                                    TaskStatus::Succeeded
                                        | TaskStatus::Failed
                                        | TaskStatus::TimedOut
                                        | TaskStatus::Cancelled
                                )
                            })
                            .filter_map(|task| task.query_id.clone())
                            .collect::<std::collections::BTreeSet<_>>();
                        self.tasks.values_mut().find_map(|task| {
                            (task.status == TaskStatus::Succeeded
                                && task.query_id.as_deref() == Some(latest_query_id)
                                && !task.answer_released
                                && task
                                    .required_query_ids
                                    .iter()
                                    .all(|id| terminal_queries.contains(id)))
                            .then(|| {
                                let output = task.completed_output.clone()?;
                                task.answer_released = true;
                                Some((task.task_id.clone(), task.run_id.clone(), output))
                            })?
                        })
                    });
                    if let Some((release_task_id, release_run_id, release_output)) = release {
                        let user_question = latest_node
                            .as_ref()
                            .and_then(|id| self.conversation_graph.nodes.get(id))
                            .map(|node| node.content.clone());
                        let graph_context = latest_query_id
                            .as_ref()
                            .and_then(|id| self.read_graph_reference_context(id, &[], true));
                        let agent_tx = self.agent_tx.clone();
                        if let Some(tx) = agent_tx {
                            self.set_llm_speaker_state(false, true);
                            let _ = tx
                                .send(AgentCmd::ToolResult {
                                    run_id: release_run_id,
                                    task_id: release_task_id,
                                    query_id: latest_query_id,
                                    outcome: Ok(release_output),
                                    user_question,
                                    graph_context,
                                })
                                .await;
                        }
                        return;
                    }

                    if let Some(tx) = &self.agent_tx {
                        let _ = tx
                            .send(AgentCmd::ToolResult {
                                run_id,
                                task_id: task_id.clone(),
                                query_id: None,
                                outcome: Err(error),
                                user_question: None,
                                graph_context: None,
                            })
                            .await;
                    }
                }
            }
        }
    }
}
