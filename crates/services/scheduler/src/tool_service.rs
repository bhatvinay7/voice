use crate::tools_grpc::tool_execution_service_server::ToolExecutionService;
use crate::tools_grpc::{ToolRequest, ToolResponse};
use tonic::{Request, Response, Status};

pub struct ToolExecutionServiceImpl {
    pub db: db::DatabaseStore,
}

#[tonic::async_trait]
impl ToolExecutionService for ToolExecutionServiceImpl {
    async fn execute_tool(
        &self,
        request: Request<ToolRequest>,
    ) -> Result<Response<ToolResponse>, Status> {
        let req = request.into_inner();
        let args: serde_json::Value =
            serde_json::from_str(&req.args_json).unwrap_or(serde_json::json!({}));

        let output = match req.tool_name.as_str() {
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
                let rag = rag::RAGPipeline::new(self.db.clone());
                let knowledge_matches = rag.search_knowledge(query);
                let results = rag.search(query);
                let cfg =
                    config::AppConfig::load().unwrap_or_else(|_| config::AppConfig::load_default());

                let msg = if rag::asks_listing_price(query) {
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
                    // Dynamically retrieved from RAG vector / knowledge document base
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
                let idemp = format!("idemp-{}", req.task_id);
                let res = self
                    .db
                    .book_viewing_slot(listing_id, user_id, slot_time, &idemp);
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
            _ => {
                let msg = format!("Action completed for inquiry: {}", args);
                serde_json::json!({ "success": true, "response_text": msg })
            }
        };

        Ok(Response::new(ToolResponse {
            success: true,
            result_json: output.to_string(),
            error_message: String::new(),
        }))
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
