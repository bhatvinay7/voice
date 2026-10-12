use std::env;
use std::path::Path;
use tonic::transport::{Channel, Endpoint};
use orchestrator::grpc::tools::tool_execution_service_client::ToolExecutionServiceClient;

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

/// Retrieves the gRPC scheduler endpoint dynamically from the environment.
pub fn get_scheduler_endpoint() -> String {
    env::var("SCHEDULER_ENDPOINT").unwrap_or_else(|_| {
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

/// Retrieves the HTTP scheduler metrics URL dynamically from the environment.
pub fn get_metrics_url() -> String {
    env::var("METRICS_URL").unwrap_or_else(|_| {
        let port = env::var("METRICS_PORT")
            .or_else(|_| env::var("SCHEDULER_METRICS_PORT"))
            .unwrap_or_else(|_| "9090".to_string());
        format!("http://127.0.0.1:{}", port)
    })
}

/// Creates a lazily connected gRPC tool client configured from environment variables.
pub fn create_test_tool_client() -> ToolExecutionServiceClient<Channel> {
    let endpoint_str = get_scheduler_endpoint();
    let channel = Endpoint::from_shared(endpoint_str)
        .unwrap_or_else(|_| Endpoint::from_static("http://[::1]:50051"))
        .connect_lazy();
    ToolExecutionServiceClient::new(channel)
}
