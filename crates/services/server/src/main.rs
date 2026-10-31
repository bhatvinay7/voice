use config::AppConfig;
use db::DatabaseStore;
use orchestrator::{Orchestrator, ProtectedReceiver, QueueEvent, RunId};
use pubsub::{create_shared_bus, PubSubBus};
use std::{net::SocketAddr, sync::Arc};
use tokio::sync::{mpsc, Mutex};

mod api;

#[derive(Clone)]
pub struct AppState {
    pub slots: orchestrator::slots::Slots,
    pub phone_media: orchestrator::phone_media::PhoneMediaRegistry,
    pub db: DatabaseStore,
    pub orch_tx: mpsc::Sender<QueueEvent>,
    pub config: AppConfig,
    pub pubsub: Arc<PubSubBus>,
    pub rag: Arc<rag::RAGPipeline>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok(); // Load .env file
    tracing_subscriber::fmt::init();
    crate::api::rbac::init_crypto_provider();

    let cfg = AppConfig::load().expect("Failed to load environment variables");
    let db = DatabaseStore::new();
    db.initialize().await.expect("Neon database unavailable or migrations missing");
    let pubsub = create_shared_bus(100);

    // 1. Start Upstash Redis Streams -> PostgreSQL Async Worker Group

    // 2. Setup MPSC Channels & Protected Receivers for Panic Resilience
    let (orch_tx, orch_rx) = mpsc::channel::<QueueEvent>(1024);
    let protected_orch_rx: ProtectedReceiver<QueueEvent> = Arc::new(Mutex::new(orch_rx));



    // Set up missing channels for Orchestrator
    let scheduler_endpoint_str = std::env::var("SCHEDULER_ENDPOINT")
        .unwrap_or_default();
    let channel = tonic::transport::Endpoint::from_shared(scheduler_endpoint_str)
        .expect("Invalid SCHEDULER_ENDPOINT")
        .connect_lazy();
    let tool_client =
        orchestrator::grpc::tools::tool_execution_service_client::ToolExecutionServiceClient::new(
            channel,
        );

    let (db_tx, db_rx) = mpsc::channel(1024);
    let protected_db_rx: db::actor::ProtectedDbReceiver = Arc::new(Mutex::new(db_rx));
    
    // Initialize sled embedded DB (uses /data in kubernetes, falls back to local dir or temp dir)
    let local_db = sled::open("/data/orchestrator_db")
        .or_else(|_| sled::open(".local_data/orchestrator_db"))
        .or_else(|_| sled::open(std::env::temp_dir().join("orchestrator_db")))?;

    // Connect to Neo4j / Memgraph Graph DB from .env credentials
    let neo4j_graph: Option<Arc<db::neo4rs::Graph>> = if let (Some(uri), Some(user), Some(pass)) = (
        cfg.neo4j_uri.as_deref(),
        cfg.neo4j_username.as_deref(),
        cfg.neo4j_password.as_deref(),
    ) {
        match db::connect_graph_db(uri, user, pass, cfg.neo4j_database.as_deref()).await {
            Ok(graph) => {
                tracing::info!("[GraphDB] Connected to Neo4j at {}", uri);
                // Push workflow DAG on startup
                let workflow_nodes = db.get_graph_nodes().await?;
                match db::push_workflow_dag_to_graph(&graph, &workflow_nodes).await {
                    Ok(_) => tracing::info!("[GraphDB] Pushed {} workflow nodes to Graph DB", workflow_nodes.len()),
                    Err(e) => tracing::warn!("[GraphDB] Could not push workflow DAG: {}", e),
                }
                Some(graph)
            }
            Err(e) => {
                tracing::warn!("[GraphDB] Could not connect to Neo4j ({}). Using Neon workflow storage.", e);
                None
            }
        }
    } else {
        tracing::info!("[GraphDB] Neo4j credentials not set in .env. Using Neon workflow storage.");
        None
    };

    let db_actor = db::actor::DbActor::new(db.clone(), neo4j_graph, local_db.clone());
    tokio::spawn(db::actor::DbActor::run_loop(protected_db_rx, db_actor));


    // 3. Initialize Orchestrator Core & LLM Agent Factory
    let db_clone = db.clone();
    let pubsub_clone = pubsub.clone();
    let agent_factory = Arc::new(move |run_id: RunId, orch_tx: mpsc::Sender<QueueEvent>| {
        let (agent_tx, agent_rx) = mpsc::channel(100);
        let agent = agent::LlmAgentActor::new(run_id, db_clone.clone(), pubsub_clone.clone(), orch_tx);
        tokio::spawn(async move {
            agent::LlmAgentActor::run_agent_loop(agent_rx, agent).await;
        });
        agent_tx
    });

    let orchestrator = Orchestrator::new(
        pubsub.clone(),
        db.clone(),
        local_db.clone(),
        tool_client,
        orch_tx.clone(),
        db_tx,
        agent_factory,
    );

    let phone_media = orchestrator.phone_media.clone();
    let slots = orchestrator.slots.clone();

    // 4. Spawn Panic-Safe Tokio Event Loop Actors
    tokio::spawn(Orchestrator::run_dispatcher(
        protected_orch_rx,
        orchestrator,
    ));

    // Start Cell Connection Manager
    let scheduler_endpoint = std::env::var("SCHEDULER_ENDPOINT")
        .unwrap_or_default();
    let cell_id = std::env::var("POD_NAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_else(|_| "cell-dev-1".to_string());
    let conn_mgr = orchestrator::connection::ConnectionManager::new(
        cell_id, scheduler_endpoint, orch_tx.clone(), slots.clone()
    );
    tokio::spawn(async move {
        conn_mgr.run().await;
    });



    let metrics_slots = slots.clone();
    tokio::spawn(async move {
        let port = std::env::var("CELL_METRICS_PORT").ok().and_then(|v| v.parse::<u16>().ok()).unwrap_or(9091);
        let app = axum::Router::new().route("/metrics", axum::routing::get(move || {
            let slots = metrics_slots.clone();
            async move { format!("# TYPE voice_agent_slots_total gauge\nvoice_agent_slots_total {}\n# TYPE voice_agent_slots_used gauge\nvoice_agent_slots_used {}\n", slots.capacity, slots.used()) }
        }));
        match tokio::net::TcpListener::bind((std::net::Ipv4Addr::UNSPECIFIED, port)).await {
            Ok(listener) => { if let Err(error) = axum::serve(listener, app).await { tracing::error!(%error, "cell metrics server failed"); } }
            Err(error) => tracing::error!(%error, "cell metrics listener failed"),
        }
    });

    let state = AppState {
        slots,
        phone_media,
        db: db.clone(),
        orch_tx,
        config: cfg.clone(),
        pubsub: pubsub.clone(),
        rag: Arc::new(rag::RAGPipeline::new(db.clone())),
    };

    let app = api::api_routes().with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], cfg.port));
    println!("[Axum Server] Listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

