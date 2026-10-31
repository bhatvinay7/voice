use crate::{DatabaseStore, AdminGraphNode};
use std::panic::AssertUnwindSafe;
use futures_util::FutureExt;
use tokio::sync::{mpsc, oneshot, Mutex};
use std::sync::Arc;
use neo4rs::Graph;

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
}

impl DbActor {
    pub fn new(store: DatabaseStore, graph: Option<Arc<Graph>>, _local_db: sled::Db) -> Self {
        Self { store, graph }
    }

    async fn check_idempotency(&self, key: &str) -> Result<Option<Vec<u8>>, String> {
        sqlx::query_scalar("SELECT response FROM public.actor_idempotency WHERE key = $1")
            .bind(key).fetch_optional(&self.store.pool).await.map_err(|_| "Could not read retry state".into())
    }

    async fn set_idempotency(&self, key: &str, value: &[u8]) -> Result<(), String> {
        sqlx::query("INSERT INTO public.actor_idempotency (key, response) VALUES ($1, $2) ON CONFLICT (key) DO NOTHING")
            .bind(key).bind(value).execute(&self.store.pool).await.map_err(|_| "Could not save retry state")?;
        Ok(())
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
                let cached = match self.check_idempotency(&idempotency_key).await {
                    Ok(cached) => cached,
                    Err(error) => { let _ = reply_to.send(Err(error)); return; }
                };
                if let Some(cached) = cached {
                    if let Ok(res) = serde_json::from_slice::<Result<Vec<AdminGraphNode>, String>>(&cached) {
                        let _ = reply_to.send(res);
                        return;
                    }
                }

                let res = self.store.get_graph_nodes().await;
                let _ = reply_to.send(res);
            }
            DbRequest::AddGraphNode { idempotency_key, node, reply_to } => {
                let cached = match self.check_idempotency(&idempotency_key).await {
                    Ok(cached) => cached,
                    Err(error) => { let _ = reply_to.send(Err(error)); return; }
                };
                if let Some(cached) = cached {
                    if let Ok(res) = serde_json::from_slice::<Result<(), String>>(&cached) {
                        let _ = reply_to.send(res);
                        return;
                    }
                }

                let res = self.store.add_graph_node(node).await;
                if let Ok(bytes) = serde_json::to_vec(&res) {
                    if res.is_ok() {
                    let _ = self.set_idempotency(&idempotency_key, &bytes).await;
                    }
                }
                let _ = reply_to.send(res);
            }
            DbRequest::PushRedisStream { idempotency_key, stream_name, session_id, trace_id, event_type, payload_json, reply_to } => {
                let cached = match self.check_idempotency(&idempotency_key).await {
                    Ok(cached) => cached,
                    Err(error) => { let _ = reply_to.send(Err(error)); return; }
                };
                if let Some(cached) = cached {
                    if let Ok(res) = serde_json::from_slice::<Result<String, String>>(&cached) {
                        let _ = reply_to.send(res);
                        return;
                    }
                }

                let res = self.store.push_redis_stream(&stream_name, &session_id, &trace_id, &event_type, &payload_json).await;
                if let Ok(bytes) = serde_json::to_vec(&res) {
                    if res.is_ok() {
                    let _ = self.set_idempotency(&idempotency_key, &bytes).await;
                    }
                }
                let _ = reply_to.send(res);
            }
            DbRequest::SetJsonWithTtl { idempotency_key, key, payload_json, ttl_seconds, reply_to } => {
                let cached = match self.check_idempotency(&idempotency_key).await {
                    Ok(cached) => cached,
                    Err(error) => { let _ = reply_to.send(Err(error)); return; }
                };
                if let Some(cached) = cached {
                    if let Ok(res) = serde_json::from_slice::<Result<(), String>>(&cached) {
                        let _ = reply_to.send(res);
                        return;
                    }
                }

                let res = self.store.set_json_with_ttl(&key, &payload_json, ttl_seconds).await;
                if let Ok(bytes) = serde_json::to_vec(&res) {
                    if res.is_ok() {
                    let _ = self.set_idempotency(&idempotency_key, &bytes).await;
                    }
                }
                let _ = reply_to.send(res);
            }
            DbRequest::SaveS3Endpoints { idempotency_key, session_id, audio_url, text_url, reply_to } => {
                let cached = match self.check_idempotency(&idempotency_key).await {
                    Ok(cached) => cached,
                    Err(error) => { let _ = reply_to.send(Err(error)); return; }
                };
                if let Some(cached) = cached {
                    if let Ok(res) = serde_json::from_slice::<Result<(), String>>(&cached) {
                        let _ = reply_to.send(res);
                        return;
                    }
                }

                let res = self.store.save_call_endpoints(&session_id, &audio_url, &text_url).await;
                if let Ok(bytes) = serde_json::to_vec(&res) {
                    if res.is_ok() {
                    let _ = self.set_idempotency(&idempotency_key, &bytes).await;
                    }
                }
                let _ = reply_to.send(res);
            }
        }
    }
}
