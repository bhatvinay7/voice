use orchestrator::grpc::tools::tool_execution_service_client::ToolExecutionServiceClient;
use serde_json::{json, Value};
use std::env;
use std::path::Path;
use tonic::transport::{Channel, Endpoint};

/// Helper function to load environment variables from .env or .env.test.
/// Follows the rule: "fetch env from .env by using proper import, do not hardcode"
pub fn load_test_env() {
    let candidate_paths = [
        ".env",
        "../.env",
        "../../.env",
        ".env.test",
        "../.env.test",
        "../../.env.test",
    ];

    for path_str in candidate_paths {
        if Path::new(path_str).exists() {
            let _ = dotenvy::from_filename(path_str);
        }
    }
}

/// Retrieves the gRPC control-plane endpoint dynamically from the environment.
pub fn get_control_plane_endpoint() -> String {
    env::var("CONTROL_PLANE_ENDPOINT").unwrap_or_else(|_| {
        let port = env::var("GRPC_PORT").unwrap_or_else(|_| "50051".to_string());
        format!("http://[::1]:{}", port)
    })
}

/// Retrieves the HTTP backend server URL dynamically from the environment.
pub fn get_server_url() -> String {
    env::var("SERVER_URL").unwrap_or_else(|_| {
        let port = env::var("PORT").unwrap_or_else(|_| "4000".to_string());
        format!("http://127.0.0.1:{}", port)
    })
}

/// Retrieves the HTTP control-plane metrics URL dynamically from the environment.
pub fn get_metrics_url() -> String {
    env::var("METRICS_URL").unwrap_or_else(|_| {
        let port = env::var("METRICS_PORT")
            .or_else(|_| env::var("CONTROL_PLANE_METRICS_PORT"))
            .unwrap_or_else(|_| "9090".to_string());
        format!("http://127.0.0.1:{}", port)
    })
}

/// Creates a lazily connected gRPC tool client configured from environment variables.
pub fn create_test_tool_client() -> ToolExecutionServiceClient<Channel> {
    let endpoint_str = get_control_plane_endpoint();
    let channel = Endpoint::from_shared(endpoint_str)
        .unwrap_or_else(|_| Endpoint::from_static("http://[::1]:50051"))
        .connect_lazy();
    ToolExecutionServiceClient::new(channel)
}

/// Test-only mirror of the current control-plane contract. It intentionally calls
/// the same database/RAG APIs as the control-plane instead of the removed
/// orchestrator-local executor.
pub async fn execute_current_tool_fixture(
    db: &db::DatabaseStore,
    tool_name: &str,
    args: &Value,
    task_id: &str,
) -> Value {
    match tool_name {
        "search_properties" => {
            let query = args
                .get("query")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let rag = rag::RAGPipeline::new(db.clone());
            let results = rag.search(query).await.unwrap_or_default();
            let listings = results
                .iter()
                .map(|result| result.listing.clone())
                .collect::<Vec<_>>();
            json!({
                "success": true,
                "listings": listings,
                "total_found": results.len(),
                "response_text": results.first().map(|result| result.listing.title.clone()).unwrap_or_default(),
            })
        }
        "book_viewing_slot" => {
            let listing_id = args
                .get("listing_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let user_id = args
                .get("user_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let slot_time = args
                .get("slot_time")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let idempotency_key = format!("test-{task_id}");
            match db
                .book_viewing_slot(listing_id, user_id, slot_time, &idempotency_key)
                .await
            {
                Ok(result) => json!({
                    "success": result.success,
                    "response_text": result.error.unwrap_or_else(|| "Viewing request processed.".to_string()),
                }),
                Err(error) => json!({ "success": false, "response_text": error }),
            }
        }
        _ => json!({ "success": false, "response_text": "Unsupported test tool" }),
    }
}
