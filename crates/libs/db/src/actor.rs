use crate::{DatabaseStore, AdminGraphNode};
use std::panic::AssertUnwindSafe;
use futures_util::FutureExt;
use tokio::sync::{mpsc, oneshot, Mutex};
use std::sync::Arc;
use neo4rs::{Graph, query};

pub type ProtectedDbReceiver = Arc<Mutex<mpsc::Receiver<DbRequest>>>;

#[derive(Debug)]
pub enum DbRequest {
    GetGraphNodes {
        idempotency_key: String,
        reply_to: oneshot::Sender<Result<Vec<AdminGraphNode>, String>>,
    },
    AddGraphNode {
        idempotency_key: String,
        node: AdminGraphNode,
        reply_to: oneshot::Sender<Result<(), String>>,
    },
    PushRedisStream {
        idempotency_key: String,
        stream_name: String,
        session_id: String,
        trace_id: String,
        event_type: String,
        payload_json: String,
        reply_to: oneshot::Sender<Result<String, String>>,
    },
    SetJsonWithTtl {
        idempotency_key: String,
        key: String,
        payload_json: String,
        ttl_seconds: usize,
        reply_to: oneshot::Sender<Result<(), String>>,
    },
    SaveS3Endpoints {
        idempotency_key: String,
        session_id: String,
        audio_url: String,
        text_url: String,
        reply_to: oneshot::Sender<Result<(), String>>,
    },
}

pub struct DbActor {
    pub store: DatabaseStore,
    pub graph: Option<Arc<Graph>>,
    pub local_db: sled::Db,
}

impl DbActor {
    pub fn new(store: DatabaseStore, graph: Option<Arc<Graph>>, local_db: sled::Db) -> Self {
        Self { store, graph, local_db }
    }

    async fn check_idempotency(&self, key: &str) -> Option<Vec<u8>> {
        self.local_db.get(format!("idemp:{}", key).as_bytes()).unwrap_or(None).map(|v| v.to_vec())
    }

    async fn set_idempotency(&self, key: &str, value: &[u8]) {
        let _ = self.local_db.insert(format!("idemp:{}", key).as_bytes(), value);
        let _ = self.local_db.flush();
    }

    pub async fn run_loop(
        protected_rx: ProtectedDbReceiver,
        actor: DbActor,
    ) {
        let db_actor = Arc::new(actor);

        loop {
            let fut = AssertUnwindSafe(async {
                let mut rx = protected_rx.lock().await;
                while let Some(req) = rx.recv().await {
                    let actor_clone = db_actor.clone();
                    tokio::spawn(async move {
                        actor_clone.handle_request(req).await;
                    });
                }
            });

            if let Err(_) = fut.catch_unwind().await {
                eprintln!("[DbActor] Panic intercepted safely! Channel receiver preserved.");
            }
        }
    }

    async fn handle_request(&self, req: DbRequest) {
        match req {
            DbRequest::GetGraphNodes { idempotency_key, reply_to } => {
                if let Some(cached) = self.check_idempotency(&idempotency_key).await {
                    if let Ok(res) = serde_json::from_slice::<Result<Vec<AdminGraphNode>, String>>(&cached) {
                        let _ = reply_to.send(res);
                        return;
                    }
                }

                if let Some(graph) = &self.graph {
                    // Try to fetch from neo4j
                    let mut nodes = Vec::new();
                    if let Ok(mut result) = graph.execute(query("MATCH (n:WorkflowNode) RETURN n.id as id, n.step_type as step_type, n.instruction as instruction, n.status as status, n.is_preset as is_preset ORDER BY n.id")).await {
                        while let Ok(Some(row)) = result.next().await {
                            let id: String = row.get("id").unwrap_or_default();
                            let step_type: String = row.get("step_type").unwrap_or_default();
                            let instruction: String = row.get("instruction").unwrap_or_default();
                            let status: String = row.get("status").unwrap_or_default();
                            let is_preset: bool = row.get("is_preset").unwrap_or(true);
                            nodes.push(AdminGraphNode { id, step_type, instruction, status, is_preset });
                        }
                    }
                    if !nodes.is_empty() {
                        let res = Ok(nodes);
                        if let Ok(bytes) = serde_json::to_vec(&res) {
                            self.set_idempotency(&idempotency_key, &bytes).await;
                        }
                        let _ = reply_to.send(res);
                        return;
                    }
                }
                
                // Fallback
                let nodes = self.store.get_graph_nodes();
                let res = Ok(nodes);
                if let Ok(bytes) = serde_json::to_vec(&res) {
                    self.set_idempotency(&idempotency_key, &bytes).await;
                }
                let _ = reply_to.send(res);
            }
            DbRequest::AddGraphNode { idempotency_key, node, reply_to } => {
                if let Some(cached) = self.check_idempotency(&idempotency_key).await {
                    if let Ok(res) = serde_json::from_slice::<Result<(), String>>(&cached) {
                        let _ = reply_to.send(res);
                        return;
                    }
                }

                if let Some(graph) = &self.graph {
                    let q = query("MERGE (n:WorkflowNode {id: $id}) SET n.step_type = $step_type, n.instruction = $instruction, n.status = $status, n.is_preset = $is_preset")
                        .param("id", node.id.clone())
                        .param("step_type", node.step_type.clone())
                        .param("instruction", node.instruction.clone())
                        .param("status", node.status.clone())
                        .param("is_preset", node.is_preset);
                    let _ = graph.run(q).await;
                }

                self.store.add_graph_node(node);
                let res = Ok(());
                if let Ok(bytes) = serde_json::to_vec(&res) {
                    self.set_idempotency(&idempotency_key, &bytes).await;
                }
                let _ = reply_to.send(res);
            }
            DbRequest::PushRedisStream { idempotency_key, stream_name, session_id, trace_id, event_type, payload_json, reply_to } => {
                if let Some(cached) = self.check_idempotency(&idempotency_key).await {
                    if let Ok(res) = serde_json::from_slice::<Result<String, String>>(&cached) {
                        let _ = reply_to.send(res);
                        return;
                    }
                }

                let res = self.store.push_redis_stream(&stream_name, &session_id, &trace_id, &event_type, &payload_json).await;
                if let Ok(bytes) = serde_json::to_vec(&res) {
                    self.set_idempotency(&idempotency_key, &bytes).await;
                }
                let _ = reply_to.send(res);
            }
            DbRequest::SetJsonWithTtl { idempotency_key, key, payload_json, ttl_seconds, reply_to } => {
                if let Some(cached) = self.check_idempotency(&idempotency_key).await {
                    if let Ok(res) = serde_json::from_slice::<Result<(), String>>(&cached) {
                        let _ = reply_to.send(res);
                        return;
                    }
                }

                let res = self.store.set_json_with_ttl(&key, &payload_json, ttl_seconds).await;
                if let Ok(bytes) = serde_json::to_vec(&res) {
                    self.set_idempotency(&idempotency_key, &bytes).await;
                }
                let _ = reply_to.send(res);
            }
            DbRequest::SaveS3Endpoints { idempotency_key, session_id, audio_url, text_url, reply_to } => {
                if let Some(cached) = self.check_idempotency(&idempotency_key).await {
                    if let Ok(res) = serde_json::from_slice::<Result<(), String>>(&cached) {
                        let _ = reply_to.send(res);
                        return;
                    }
                }

                let res = self.store.save_call_endpoints(&session_id, &audio_url, &text_url).await;
                if let Ok(bytes) = serde_json::to_vec(&res) {
                    self.set_idempotency(&idempotency_key, &bytes).await;
                }
                let _ = reply_to.send(res);
            }
        }
    }
}
