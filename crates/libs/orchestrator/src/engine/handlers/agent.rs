use super::super::session::SessionActor;
use super::executor::execute_tool;
use crate::models::*;
use pubsub::SystemEvent;
use serde_json::json;
use std::time::Duration;

impl SessionActor {
    pub async fn handle_agent_event(&mut self, event: AgentEvent) {
        match event {
            AgentEvent::ToolRequested {
                run_id,
                task_id,
                tool_name,
                args,
            } => {
                let task_record = TaskStateRecord {
                    task_id: task_id.clone(),
                    run_id: run_id.clone(),
                    tool_name: tool_name.clone(),
                    args: args.clone(),
                    status: TaskStatus::Running,
                    attempt: 1,
                    max_retry: 3,
                    retry_count: 0,
                    timeout_duration: Duration::from_secs(5),
                    error: None,
                };

                self.tasks.insert(task_id.clone(), task_record);
                self.register_timeout(task_id.clone(), Duration::from_secs(5));

                let client = self.orch.tool_client.clone();
                let q_tx = self.orch.queue_tx.clone();
                let db_local = self.orch.db.clone();
                let r_id = run_id.0.clone();
                let t_id = task_id.clone();
                let t_name = tool_name.clone();

                tokio::spawn(async move {
                    execute_tool(client, db_local, q_tx, r_id, t_id, t_name, args, 1).await;
                });
            }

            AgentEvent::Finished { run_id, result } => {
                if let Some(graph) = &mut self.workflow_graph {
                    graph.status = "Completed".to_string();
                }

                if let Some(s3_tx) = &self.s3_dump_tx {
                    if s3_tx
                        .try_send(S3DumpData::Text {
                            session_id: run_id.call_id().to_string(),
                            payload: format!("agent_response: {}", result),
                        })
                        .is_err()
                    {
                        tracing::trace!(
                            "Session {} recording text queue full; dropping agent response copy",
                            run_id.0
                        );
                    }
                }

                if let Some(sts_tx) = &self.sts_tx {
                    let _ = sts_tx
                        .send(crate::models::StsPipelineCmd::InjectContext {
                            text: result.clone(),
                        })
                        .await;
                }

                let task_id_cache = TaskId(format!("task-db-cache-{}", run_id.0));
                self.dispatch_db_action(
                    task_id_cache,
                    PendingDbAction::SetJsonWithTtl {
                        key: format!("cache:llm:{}", run_id.call_id()),
                        payload_json: json!({ "response": result }).to_string(),
                        ttl_seconds: 1500,
                    },
                    false,
                )
                .await;
            }

            AgentEvent::Failed { run_id, error } => {
                if let Some(graph) = &mut self.workflow_graph {
                    graph.status = "Failed".to_string();
                }

                let fallback_text = "I apologize, but I had trouble processing that request. Please let me know how I can assist you with luxury apartments, viewing slots, or tenancy contracts in the UAE.".to_string();

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

                if let Some(sts_tx) = &self.sts_tx {
                    let _ = sts_tx
                        .send(crate::models::StsPipelineCmd::InjectContext {
                            text: fallback_text,
                        })
                        .await;
                }
            }

            AgentEvent::NeedsUserInput {
                run_id: _,
                question,
            } => {
                if let Some(sts_tx) = &self.sts_tx {
                    let _ = sts_tx
                        .send(crate::models::StsPipelineCmd::InjectContext { text: question })
                        .await;
                }
            }
        }
    }
}
