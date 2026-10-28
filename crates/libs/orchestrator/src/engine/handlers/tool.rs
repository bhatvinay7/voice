use super::super::session::SessionActor;
use super::executor::execute_tool;
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
            } => {
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
                }
            }

            ToolEvent::Done {
                task_id, output, ..
            } => {
                self.clear_timeout(&task_id);

                let (run_id_opt, tool_name_opt) = if let Some(task) = self.tasks.get_mut(&task_id) {
                    task.status = TaskStatus::Succeeded;
                    (Some(task.run_id.clone()), Some(task.tool_name.clone()))
                } else {
                    (None, None)
                };

                if let (Some(run_id), Some(tool_name)) = (run_id_opt, tool_name_opt) {
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

                    if let Some(tx) = &self.agent_tx {
                        let _ = tx
                            .send(AgentCmd::ToolResult {
                                run_id: run_id.clone(),
                                task_id: task_id.clone(),
                                outcome: Ok(output),
                            })
                            .await;
                    }
                }
            }

            ToolEvent::Failed {
                task_id,
                error,
                retryable,
                ..
            } => {
                self.clear_timeout(&task_id);

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
                    self.register_timeout(task_id.clone(), timeout_dur);
                    let call_id = run_id_str.strip_prefix("run-").unwrap_or(&run_id_str);
                    if !call_id.starts_with("phone:") {
                        let _ = self.orch.pubsub.publish(SystemEvent::AgentToolExecute {
                            call_id: call_id.to_string(),
                            tool_name: tool_name.clone(),
                            args: args.clone(),
                            idempotency_key: task_id.0.clone(),
                        });
                    }

                    let client = self.orch.tool_client.clone();
                    let q_tx = self.orch.queue_tx.clone();
                    let db_local = self.orch.db.clone();
                    let t_id = task_id.clone();
                    let r_id = run_id_str.clone();

                    tokio::spawn(async move {
                        execute_tool(client, db_local, q_tx, r_id, t_id, tool_name, args, attempt)
                            .await;
                    });
                } else if let Some(run_id) = self.tasks.get(&task_id).map(|t| t.run_id.clone()) {
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

                    if let Some(tx) = &self.agent_tx {
                        let _ = tx
                            .send(AgentCmd::ToolResult {
                                run_id,
                                task_id: task_id.clone(),
                                outcome: Err(error),
                            })
                            .await;
                    }
                }
            }
        }
    }
}
