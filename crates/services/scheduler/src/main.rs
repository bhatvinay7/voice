use axum::{routing::get, Router};
use prometheus::{Encoder, TextEncoder};
use scheduler::{CellControlService, SchedulerState};
use tonic::transport::Server;
use scheduler::grpc::cell_control_server::CellControlServer;
use std::net::SocketAddr;

async fn metrics_handler() -> String {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = vec![];
    if let Err(e) = encoder.encode(&metric_families, &mut buffer) {
        tracing::error!("Failed to encode metrics: {:?}", e);
    }
    String::from_utf8(buffer).unwrap_or_default()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    println!("[Scheduler] Starting scheduler service...");
    let grpc_port: u16 = std::env::var("GRPC_PORT")
        .or_else(|_| std::env::var("SCHEDULER_PORT"))
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(50051);
    let http_port: u16 = std::env::var("METRICS_PORT")
        .or_else(|_| std::env::var("SCHEDULER_METRICS_PORT"))
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(9090);
    let grpc_addr: SocketAddr = SocketAddr::from(([0, 0, 0, 0], grpc_port));
    let http_addr: SocketAddr = SocketAddr::from(([0, 0, 0, 0], http_port));
    
    let state = SchedulerState::new();
    let service = CellControlService { state: state.clone() };

    let app = Router::new().route("/metrics", get(metrics_handler));
    
    tokio::spawn(async move {
        match tokio::net::TcpListener::bind(http_addr).await {
            Ok(listener) => {
                println!("[Scheduler] Metrics listening on {}", http_addr);
                if let Err(e) = axum::serve(listener, app).await {
                    tracing::error!("Metrics server error: {:?}", e);
                }
            }
            Err(e) => {
                tracing::error!("Failed to bind metrics listener on {}: {:?}", http_addr, e);
            }
        }
    });

    let db = db::DatabaseStore::new();
    let tool_service = scheduler::tool_service::ToolExecutionServiceImpl { db };

    Server::builder()
        .add_service(CellControlServer::new(service))
        .add_service(scheduler::tools_grpc::tool_execution_service_server::ToolExecutionServiceServer::new(tool_service))
        .serve(grpc_addr)
        .await?;

    Ok(())
}
