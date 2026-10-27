use super::orchestrator::Orchestrator;
use crate::models::*;
use futures_util::FutureExt;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub struct SessionActor {
    pub session_id: String,
    pub run_id: RunId,
    pub tasks: HashMap<TaskId, TaskStateRecord>,
    pub db_tasks: HashMap<TaskId, (usize, PendingDbAction)>,
    pub workflow_graph: Option<WorkflowGraph>,
    pub agent_tx: Option<mpsc::Sender<AgentCmd>>,
    pub sts_tx: Option<mpsc::Sender<StsPipelineCmd>>,
    pub s3_dump_tx: Option<mpsc::Sender<S3DumpData>>,
    pub tts_audio_tx: Option<mpsc::Sender<TtsAudioPacket>>,
    pub audio_broadcast_tx: tokio::sync::broadcast::Sender<(String, Vec<u8>)>,
    pub ai_listen_muted: bool,
    pub last_user_transcript: Option<(String, std::time::Instant)>,
    pub cancel_token: CancellationToken,
    pub orch: Orchestrator,
}

impl SessionActor {
    pub fn new(session_id: String, orch: Orchestrator) -> Self {
        let run_id = RunId(format!("run-{}", session_id));
        let agent_tx = (orch.agent_factory)(run_id.clone(), orch.queue_tx.clone());

        let (s3_tx, s3_rx) = mpsc::channel(1_000);
        let s_id = session_id.clone();
        tokio::spawn(async move {
            crate::supervisors::run_s3_dump_pipeline(s_id, s3_rx).await;
        });

        let (tts_audio_tx, tts_audio_rx) = mpsc::channel(256);
        let tts_session_id = session_id.clone();
        tokio::spawn(async move {
            crate::supervisors::run_tts_audio_packet_pipeline(tts_session_id, tts_audio_rx).await;
        });

        let (audio_broadcast_tx, mut tone_rx) =
            tokio::sync::broadcast::channel::<(String, Vec<u8>)>(100);
        let agent_tx_clone = agent_tx.clone();
        let r_id = run_id.clone();

        tokio::spawn(async move {
            let mut chunk_count = 0;
            let cfg =
                config::AppConfig::load().unwrap_or_else(|_| config::AppConfig::load_default());
            let tone_api_key = cfg.tone_model_api_key.clone();

            while let Ok((sess_id, _audio_chunk)) = tone_rx.recv().await {
                chunk_count += 1;
                if chunk_count % 5 == 0 {
                    let agent_tx_inner = agent_tx_clone.clone();
                    let rid_inner = r_id.clone();
                    let _api_key = tone_api_key.clone();
                    let s_id = sess_id.clone();

                    tokio::spawn(async move {
                        let fut = std::panic::AssertUnwindSafe(async {
                            let _ = agent_tx_inner
                                .send(AgentCmd::InjectToneContext {
                                    run_id: rid_inner.clone(),
                                    session_id: s_id,
                                    tone_intent: "neutral".to_string(),
                                    confidence: 0.85,
                                })
                                .await;
                        });

                        if let Err(_) = fut.catch_unwind().await {
                            eprintln!(
                                "[ToneModel] Panic intercepted during tone inference for run {}",
                                rid_inner.0
                            );
                        }
                    });
                }
            }
        });

        let (sts_event_tx, mut sts_event_rx) = mpsc::channel(100);
        let sts_worker = crate::sts_pipe::StsPipelineWorker::spawn(
            session_id.clone(),
            sts_event_tx,
            s3_tx.clone(),
        );

        let q_tx_inner = orch.queue_tx.clone();
        let session_id_clone = session_id.clone();
        tokio::spawn(async move {
            while let Some(event) = sts_event_rx.recv().await {
                let _ = q_tx_inner
                    .send(QueueEvent::StsEvent {
                        session_id: session_id_clone.clone(),
                        event,
                    })
                    .await;
            }
        });

        Self {
            run_id,
            session_id,
            tasks: HashMap::new(),
            db_tasks: HashMap::new(),
            workflow_graph: None,
            agent_tx: Some(agent_tx),
            sts_tx: Some(sts_worker.get_cmd_tx()),
            s3_dump_tx: Some(s3_tx),
            tts_audio_tx: Some(tts_audio_tx),
            audio_broadcast_tx,
            ai_listen_muted: false,
            last_user_transcript: None,
            cancel_token: CancellationToken::new(),
            orch,
        }
    }

    pub async fn enqueue_tts_audio_packet(
        &self,
        packet: TtsAudioPacket,
    ) -> Result<(), mpsc::error::SendError<TtsAudioPacket>> {
        if let Some(tx) = &self.tts_audio_tx {
            tx.send(packet).await?;
        }
        Ok(())
    }

    pub async fn run(mut self, mut rx: mpsc::Receiver<SessionEvent>) {
        println!("[SessionActor {}] Started.", self.session_id);
        loop {
            tokio::select! {
                Some(event) = rx.recv() => {
                    self.handle_queue_event(event).await;
                }
                _ = self.cancel_token.cancelled() => {
                    println!("[SessionActor {}] Cancel token triggered. Shutting down.", self.session_id);
                    break;
                }
                else => break,
            }
        }

        self.orch.session_inboxes.remove(&self.session_id);
        let _ = self
            .orch
            .local_db
            .remove(format!("session:{}", self.session_id).as_bytes());
        let _ = self.orch.local_db.flush();
        println!("[SessionActor {}] Stopped.", self.session_id);
    }

    pub fn register_inbox(&self, tx: mpsc::Sender<SessionEvent>) {
        self.orch
            .session_inboxes
            .insert(self.session_id.clone(), tx);
    }

    pub fn register_timeout(&mut self, task_id: TaskId, duration: Duration) {
        let expire_time = chrono::Utc::now().timestamp_millis() + duration.as_millis() as i64;
        let db_key = format!("timeout:{}", task_id.0);
        let _ = self
            .orch
            .local_db
            .insert(db_key.as_bytes(), &expire_time.to_be_bytes());
        let _ = self.orch.local_db.flush();

        let mut wheel = self.orch.timer_wheel.lock();
        let existing_key = wheel.keys.get(&task_id).cloned();
        if let Some(key) = existing_key {
            wheel.queue.reset(&key, duration);
        } else {
            let key = wheel.queue.insert(task_id.clone(), duration);
            wheel.keys.insert(task_id.clone(), key);
        }
        self.orch
            .task_to_session
            .insert(task_id, self.session_id.clone());
    }

    pub fn clear_timeout(&mut self, task_id: &TaskId) -> bool {
        let db_key = format!("timeout:{}", task_id.0);
        let _ = self.orch.local_db.remove(db_key.as_bytes());
        let _ = self.orch.local_db.flush();
        self.orch.task_to_session.remove(task_id);

        let mut wheel = self.orch.timer_wheel.lock();
        if let Some(key) = wheel.keys.remove(task_id) {
            wheel.queue.remove(&key);
            true
        } else {
            false
        }
    }

    pub async fn dispatch_db_action(
        &mut self,
        task_id: TaskId,
        action: PendingDbAction,
        is_retry: bool,
    ) {
        if !is_retry {
            self.db_tasks.insert(task_id.clone(), (0, action.clone()));
        }

        self.register_timeout(task_id.clone(), Duration::from_secs(5));

        let db_tx = self.orch.db_tx.clone();
        let q_tx = self.orch.queue_tx.clone();
        let t_id = task_id.clone();

        match action.clone() {
            PendingDbAction::GetGraphNodes {
                session_id,
                user_id,
                language,
            } => {
                let (tx, rx) = tokio::sync::oneshot::channel();
                let _ = db_tx
                    .send(db::actor::DbRequest::GetGraphNodes {
                        idempotency_key: task_id.0.clone(),
                        reply_to: tx,
                    })
                    .await;
                tokio::spawn(async move {
                    match rx.await {
                        Ok(Ok(nodes)) => {
                            let _ = q_tx
                                .send(QueueEvent::GraphNodesFetched {
                                    session_id,
                                    user_id,
                                    language,
                                    task_id: t_id,
                                    nodes,
                                })
                                .await;
                        }
                        Ok(Err(e)) => {
                            let _ = q_tx
                                .send(QueueEvent::DbActionCompleted {
                                    task_id: t_id,
                                    action: "GetGraphNodes".to_string(),
                                    success: false,
                                    error: Some(e),
                                })
                                .await;
                        }
                        Err(e) => {
                            let _ = q_tx
                                .send(QueueEvent::DbActionCompleted {
                                    task_id: t_id,
                                    action: "GetGraphNodes".to_string(),
                                    success: false,
                                    error: Some(e.to_string()),
                                })
                                .await;
                        }
                    }
                });
            }
            PendingDbAction::PushRedisStream {
                stream_name,
                session_id,
                trace_id,
                event_type,
                payload_json,
            } => {
                let (tx, rx) = tokio::sync::oneshot::channel();
                let _ = db_tx
                    .send(db::actor::DbRequest::PushRedisStream {
                        idempotency_key: task_id.0.clone(),
                        stream_name,
                        session_id,
                        trace_id,
                        event_type,
                        payload_json,
                        reply_to: tx,
                    })
                    .await;
                tokio::spawn(async move {
                    let (success, error) = match rx.await {
                        Ok(Ok(_)) => (true, None),
                        Ok(Err(e)) => (false, Some(e)),
                        Err(e) => (false, Some(e.to_string())),
                    };
                    let _ = q_tx
                        .send(QueueEvent::DbActionCompleted {
                            task_id: t_id,
                            action: "PushRedisStream".to_string(),
                            success,
                            error,
                        })
                        .await;
                });
            }
            PendingDbAction::SetJsonWithTtl {
                key,
                payload_json,
                ttl_seconds,
            } => {
                let (tx, rx) = tokio::sync::oneshot::channel();
                let _ = db_tx
                    .send(db::actor::DbRequest::SetJsonWithTtl {
                        idempotency_key: task_id.0.clone(),
                        key,
                        payload_json,
                        ttl_seconds,
                        reply_to: tx,
                    })
                    .await;
                tokio::spawn(async move {
                    let (success, error) = match rx.await {
                        Ok(Ok(_)) => (true, None),
                        Ok(Err(e)) => (false, Some(e)),
                        Err(e) => (false, Some(e.to_string())),
                    };
                    let _ = q_tx
                        .send(QueueEvent::DbActionCompleted {
                            task_id: t_id,
                            action: "SetJsonWithTtl".to_string(),
                            success,
                            error,
                        })
                        .await;
                });
            }
            PendingDbAction::SaveS3Endpoints {
                session_id,
                audio_url,
                text_url,
            } => {
                let (tx, rx) = tokio::sync::oneshot::channel();
                let _ = db_tx
                    .send(db::actor::DbRequest::SaveS3Endpoints {
                        idempotency_key: task_id.0.clone(),
                        session_id,
                        audio_url,
                        text_url,
                        reply_to: tx,
                    })
                    .await;
                tokio::spawn(async move {
                    let (success, error) = match rx.await {
                        Ok(Ok(_)) => (true, None),
                        Ok(Err(e)) => (false, Some(e)),
                        Err(e) => (false, Some(e.to_string())),
                    };
                    let _ = q_tx
                        .send(QueueEvent::DbActionCompleted {
                            task_id: t_id,
                            action: "SaveS3Endpoints".to_string(),
                            success,
                            error,
                        })
                        .await;
                });
            }
        }
    }
}