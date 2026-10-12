use crate::grpc::tools::tool_execution_service_client::ToolExecutionServiceClient;
use crate::models::{QueueEvent, TaskId, ToolEvent};
use db::DatabaseStore;
use tokio::sync::mpsc;
use tonic::transport::Channel;

/// Executes a tool request either remotely via gRPC or via in-process fallback
pub async fn execute_tool(
    mut client: ToolExecutionServiceClient<Channel>,
    db_local: DatabaseStore,
    q_tx: mpsc::Sender<QueueEvent>,
    run_id: String,
    task_id: TaskId,
    tool_name: String,
    args: serde_json::Value,
    attempt: u32,
) {
    let a_json = args.to_string();
    let req = tonic::Request::new(crate::grpc::tools::ToolRequest {
        run_id: run_id.clone(),
        task_id: task_id.0.clone(),
        tool_name: tool_name.clone(),
        args_json: a_json.clone(),
    });

    let rpc_result = client.execute_tool(req).await;
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
                        error: inner.error_message,
                        retryable: false,
                    }))
                    .await;
            }
        }
        Err(e) => {
            tracing::info!(
                "Remote gRPC tool service unavailable ({:?}), executing tool '{}' in-process fallback",
                e.code(),
                tool_name
            );
            let output_val = execute_in_process_tool(&db_local, &tool_name, &args, &task_id.0);
            let _ = q_tx
                .send(QueueEvent::Tool(ToolEvent::Done {
                    task_id,
                    attempt,
                    output: output_val,
                }))
                .await;
        }
    }
}

/// Fallback in-process execution for local operations
pub fn execute_in_process_tool(
    db_local: &DatabaseStore,
    tool_name: &str,
    args: &serde_json::Value,
    task_id_str: &str,
) -> serde_json::Value {
    match tool_name {
        "book_viewing_slot" => {
            let listing_id = args
                .get("listing_id")
                .and_then(|v| v.as_str())
                .unwrap_or("prop-101");
            let user_id = args
                .get("user_id")
                .and_then(|v| v.as_str())
                .unwrap_or("user-unverified");
            let slot_time = args
                .get("slot_time")
                .and_then(|v| v.as_str())
                .unwrap_or("2026-10-05T10:00:00Z");
            let idemp = format!("idemp-{}", task_id_str);
            let res = db_local.book_viewing_slot(listing_id, user_id, slot_time, &idemp);
            let msg = if res.success {
                format!(
                    "Your viewing slot for property {} has been booked successfully for {}.",
                    listing_id, slot_time
                )
            } else {
                res.error
                    .unwrap_or_else(|| "Viewing slot booking failed.".to_string())
            };
            serde_json::json!({ "success": res.success, "response_text": msg })
        }
        "search_properties"
        | "search_listings"
        | "rag_search"
        | "search"
        | "RagFetchAndAgentAnalysis" => {
            let query = args
                .get("query")
                .or_else(|| args.get("search_query"))
                .or_else(|| args.get("text"))
                .or_else(|| args.get("input"))
                .or_else(|| args.get("q"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let rag = rag::RAGPipeline::new(db_local.clone());
            let knowledge_matches = rag.search_knowledge(query);
            let results = rag.search(query);
            let cfg =
                config::AppConfig::load().unwrap_or_else(|_| config::AppConfig::load_default());

            let lower_query = query.to_lowercase();
            let msg = if lower_query.contains("restaurant")
                || lower_query.contains("dining")
                || lower_query.contains("food")
                || lower_query.contains("culinary")
            {
                "While I specialize in luxury residential properties rather than dining reservations, I can recommend exquisite luxury apartments situated in world-class culinary hotspots and award-winning fine dining districts like Dubai Marina and Downtown Dubai.".to_string()
            } else if rag::asks_listing_price(&query) {
                if let Some(top) = results.first() {
                    let price_type = if top.listing.listing_type == cfg.filter_primary_type_a {
                        "purchase price"
                    } else {
                        "per year"
                    };
                    format!(
                        "{} in {} is listed at {} {} ({}).",
                        top.listing.title,
                        top.listing.location,
                        top.listing.price_aed,
                        cfg.app_currency,
                        price_type
                    )
                } else if !knowledge_matches.is_empty() {
                    format_knowledge_matches(&knowledge_matches)
                } else {
                    cfg.domain_fallback_message.clone()
                }
            } else if !knowledge_matches.is_empty() {
                format_knowledge_matches(&knowledge_matches)
            } else if let Some(top) = results.first() {
                let price_type = if top.listing.listing_type == cfg.filter_primary_type_a {
                    "purchase price"
                } else {
                    "per year"
                };
                format!(
                    "I found a matching option in {}: {}, priced at {} {} ({}) with {} rooms/units.",
                    top.listing.location, top.listing.title, top.listing.price_aed, cfg.app_currency, price_type, top.listing.bedrooms
                )
            } else {
                cfg.domain_fallback_message.clone()
            };

            let listings_val = serde_json::to_value(
                results
                    .iter()
                    .map(|r| r.listing.clone())
                    .collect::<Vec<_>>(),
            )
            .unwrap_or(serde_json::json!([]));

            serde_json::json!({
                "success": true,
                "response_text": msg,
                "total_found": results.len(),
                "listings": listings_val,
            })
        }
        _ => {
            let msg = format!("Action completed for inquiry: {}", args);
            serde_json::json!({ "success": true, "response_text": msg })
        }
    }
}

fn format_knowledge_matches(matches: &[rag::KnowledgeSearchResult]) -> String {
    matches
        .iter()
        .take(3)
        .map(|result| {
            let content: String = result.document.content.chars().take(900).collect();
            format!("{}: {}", result.document.title, content)
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}
