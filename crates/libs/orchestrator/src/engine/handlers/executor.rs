use crate::grpc::tools::tool_execution_service_client::ToolExecutionServiceClient;
use crate::models::{QueueEvent, TaskId, ToolEvent};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tonic::transport::Channel;

/// Executes a tool request through the control-plane boundary.
///
/// The orchestrator deliberately has no local implementation of tools. Keeping
/// a second path here caused tool behaviour to drift between single-process and
/// deployed installations, and made cancellation/ownership ambiguous.
pub async fn execute_tool(
    mut client: ToolExecutionServiceClient<Channel>,
    q_tx: mpsc::Sender<QueueEvent>,
    run_id: String,
    task_id: TaskId,
    tool_name: String,
    args: serde_json::Value,
    attempt: u32,
    query_id: Option<String>,
    playback_generation: u64,
    cancellation: CancellationToken,
) {
    let a_json = args.to_string();
    let req = tonic::Request::new(crate::grpc::tools::ToolRequest {
        run_id: run_id.clone(),
        task_id: task_id.0.clone(),
        tool_name: tool_name.clone(),
        args_json: a_json.clone(),
    });

    let rpc_result = tokio::select! {
        _ = cancellation.cancelled() => {
            let _ = q_tx.send(QueueEvent::Tool(ToolEvent::Failed {
                task_id,
                attempt,
                query_id,
                playback_generation,
                error: "Tool execution cancelled with its session".to_string(),
                retryable: false,
            })).await;
            return;
        }
        result = client.execute_tool(req) => result,
    };
    match rpc_result {
        Ok(resp) => {
            let inner = resp.into_inner();
            if inner.success {
                tracing::info!(
                    "Tool '{}' executed successfully via remote gRPC service",
                    tool_name
                );
                let val = serde_json::from_str(&inner.result_json)
                    .unwrap_or_else(|_| serde_json::json!({}));
                let _ = q_tx
                    .send(QueueEvent::Tool(ToolEvent::Done {
                        task_id,
                        attempt,
                        query_id,
                        playback_generation,
                        output: val,
                    }))
                    .await;
            } else {
                tracing::warn!(
                    "Tool '{}' remote gRPC execution returned failure: {}",
                    tool_name,
                    inner.error_message
                );
                let _ = q_tx
                    .send(QueueEvent::Tool(ToolEvent::Failed {
                        task_id,
                        attempt,
                        query_id,
                        playback_generation,
                        error: inner.error_message,
                        retryable: false,
                    }))
                    .await;
            }
        }
        Err(e) => {
            tracing::warn!(
                "Scheduler unavailable for tool '{}' ({:?})",
                tool_name,
                e.code()
            );
            let _ = q_tx
                .send(QueueEvent::Tool(ToolEvent::Failed {
                    task_id,
                    attempt,
                    query_id,
                    playback_generation,
                    error: "Tool scheduler is unavailable".to_string(),
                    retryable: true,
                }))
                .await;
        }
    }
}
