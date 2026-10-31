mod conversation;
use std::panic::AssertUnwindSafe;
use tokio::sync::mpsc;
use futures_util::FutureExt;
use db::DatabaseStore;

#[derive(Debug, Clone)]
pub enum TransactionTask {
    Summarize {
        session_id: String,
        transcript: String,
    },
}

#[derive(Debug, Clone)]
pub enum DbUpdateTask {
    SaveCallSummary {
        session_id: String,
        generated_summary: String,
    },
}

pub async fn run_summary_job_worker(
    mut summary_rx: mpsc::Receiver<(String, String)>,
    transaction_tx: mpsc::Sender<TransactionTask>,
) {
    while let Some((session_id, transcript)) = summary_rx.recv().await {
        if transaction_tx.send(TransactionTask::Summarize { session_id, transcript }).await.is_err() {
            break;
        }
    }
}

pub async fn run_transaction_worker(
    mut tx_rx: mpsc::Receiver<TransactionTask>,
    db_update_tx: mpsc::Sender<DbUpdateTask>,
) {
    loop {
        let fut = AssertUnwindSafe(async {
            while let Some(task) = tx_rx.recv().await {
                match task {
                    TransactionTask::Summarize { session_id, transcript } => {
                        println!("[Summary Worker] Summarizing conversation for call: {}", session_id);
                        match generate_summary(&transcript).await {
                            Ok(generated_summary) => {
                                if let Err(error) = db_update_tx
                                    .send(DbUpdateTask::SaveCallSummary {
                                        session_id: session_id.clone(),
                                        generated_summary,
                                    })
                                    .await
                                {
                                    eprintln!("[Summary Worker] Could not queue summary for {}: {}", session_id, error);
                                }
                            }
                            Err(error) => eprintln!("[Summary Worker] Could not summarize call {}: {}", session_id, error),
                        }
                    }
                }
            }
        });

        if let Err(_) = fut.catch_unwind().await {
            eprintln!("[Transaction Worker] Panic intercepted during LLM interaction! Restarting worker.");
        } else {
            break;
        }
    }
}

async fn generate_summary(transcript: &str) -> Result<String, String> {
    let cfg = config::AppConfig::load()
        .map_err(|error| format!("Could not load LLM configuration: {}", error))?;
    if cfg.llm_api_key.trim().is_empty() {
        return Err("LLM_API_KEY is empty".to_string());
    }

    let response = reqwest::Client::new()
        .post(&cfg.llm_endpoint_url)
        .bearer_auth(&cfg.llm_api_key)
        .timeout(std::time::Duration::from_secs(45))
        .json(&serde_json::json!({
            "model": cfg.llm_model,
            "temperature": 0.2,
            "messages": [
                {
                    "role": "system",
                    "content": std::env::var("SUMMARY_SYSTEM_PROMPT").unwrap_or_default()
                },
                { "role": "user", "content": transcript }
            ]
        }))
        .send()
        .await
        .map_err(|error| format!("LLM request failed: {}", error))?;
    let status = response.status();
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|error| format!("Could not parse LLM response ({}): {}", status, error))?;
    if !status.is_success() {
        return Err(format!("LLM returned {}: {}", status, body));
    }
    body.get("choices")
        .and_then(serde_json::Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|summary| !summary.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "LLM response did not contain a summary".to_string())
}

pub async fn run_db_update_worker(
    mut db_update_rx: mpsc::Receiver<DbUpdateTask>,
    db: DatabaseStore,
) {
    loop {
        let fut = AssertUnwindSafe(async {
            while let Some(task) = db_update_rx.recv().await {
                match task {
                    DbUpdateTask::SaveCallSummary {
                        session_id,
                        generated_summary,
                    } => {
                        println!("[DB Update Worker] Persisting summary for session: {}", session_id);
                        let events = match db.load_session_events(&session_id).await {
                            Ok(events) => events,
                            Err(error) => { eprintln!("Could not load call context: {error}"); continue; }
                        };
                        let user_id = events.iter().find(|event| event.event_type == "SESSION_STARTED")
                            .and_then(|event| serde_json::from_str::<serde_json::Value>(&event.payload_json).ok())
                            .and_then(|payload| payload.get("user_id").and_then(|v| v.as_str()).map(str::to_owned))
                            .unwrap_or_default();
                        let duration = events.first().zip(events.last()).and_then(|(first, last)| {
                            let start = chrono::DateTime::parse_from_rfc3339(&first.timestamp).ok()?;
                            let end = chrono::DateTime::parse_from_rfc3339(&last.timestamp).ok()?;
                            Some((end - start).num_seconds().max(0).min(u32::MAX as i64) as u32)
                        }).unwrap_or(0);
                        let endpoints = match db.get_call_endpoints(&session_id).await {
                            Ok(record) => record,
                            Err(error) => { eprintln!("Could not load recording details: {error}"); continue; }
                        };
                        if let Err(error) = db.save_call_summary(
                            &session_id,
                            &user_id,
                            duration,
                            endpoints.as_ref().map(|e| e.text_url.as_str()).unwrap_or(""),
                            endpoints.as_ref().map(|e| e.audio_url.as_str()).unwrap_or(""),
                            &generated_summary,
                            0,
                            "",
                        ).await {
                            eprintln!("[DB Update Worker] Could not persist summary for {}: {}", session_id, error);
                        } else {
                            println!("[DB Update Worker] Summary successfully committed for session: {}", session_id);
                        }
                    }
                }
            }
        });

        if let Err(_) = fut.catch_unwind().await {
            eprintln!("[DB Update Worker] Panic intercepted! Restarting worker.");
        } else {
            break;
        }
    }
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok(); // Load .env if it exists
    println!("Starting separate Async Background Worker service...");
    let db = DatabaseStore::new();
    db.initialize().await.expect("Neon database unavailable or migrations missing");

    let existing_summary_ids = db
        .load_call_summaries()
        .await
        .map(|summaries| summaries.into_iter().map(|summary| summary.call_id).collect::<std::collections::HashSet<_>>())
        .unwrap_or_default();
    match db.load_call_endpoints().await {
        Ok(recordings) => {
            for recording in recordings {
                if !existing_summary_ids.contains(&recording.session_id) {
                    if let Err(error) = db.enqueue_call_summary(&recording.session_id, 0).await {
                        eprintln!("[Worker Init] Could not enqueue existing call {} for summarization: {}", recording.session_id, error);
                    }
                }
            }
        }
        Err(error) => eprintln!("[Worker Init] Could not load existing call recordings for summary backfill: {}", error),
    }

    let (conversation_fetch_tx, conversation_fetch_rx) = mpsc::channel(100);
    let (summary_worker_tx, summary_worker_rx) = mpsc::channel(100);
    let (transaction_tx, transaction_rx) = mpsc::channel(100);
    let (db_update_tx, db_update_rx) = mpsc::channel(100);

    let cfg = config::AppConfig::load().unwrap_or_else(|_| config::AppConfig::load_default());
    let redis_url = cfg.redis_url.clone();
    
    let redis_url_poller = redis_url.clone();
    let summary_tx_poller = conversation_fetch_tx;
    
    // Delay Queue Poller Loop
    tokio::spawn(async move {
        println!("[Worker Init] Starting ZSET Delay Queue Poller...");
        if let Ok(client) = redis::Client::open(redis_url_poller) {
            if let Ok(mut con) = client.get_multiplexed_async_connection().await {
                use redis::AsyncCommands;
                loop {
                    let now = chrono::Utc::now().timestamp();
                    let ready_jobs: redis::RedisResult<Vec<String>> = con.zrangebyscore("s3_sync_delay_queue", "-inf", now).await;
                    
                    if let Ok(jobs) = ready_jobs {
                        for session_id in jobs {
                            // Atomically remove to ensure no double processing
                            let removed: redis::RedisResult<i32> = con.zrem("s3_sync_delay_queue", &session_id).await;
                            if let Ok(count) = removed {
                                if count > 0 {
                                    println!("[Worker Poller] Summary job ready for {}. Forwarding to summary worker.", session_id);
                                    if let Err(e) = summary_tx_poller.send(session_id).await {
                                        eprintln!("[Worker Poller] Could not forward call to summary worker: {}", e);
                                    }
                                }
                            }
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        }
    });

    let conversation_handle = tokio::spawn(conversation::run_conversation_fetch_worker(
        conversation_fetch_rx, summary_worker_tx, db.clone(),
    ));
    let summary_handle = tokio::spawn(run_summary_job_worker(
        summary_worker_rx,
        transaction_tx,
    ));

    let tx_handle = tokio::spawn(run_transaction_worker(
        transaction_rx,
        db_update_tx,
    ));

    let db_handle = tokio::spawn(run_db_update_worker(
        db_update_rx,
        db.clone(),
    ));

    let _ = tokio::join!(conversation_handle, summary_handle, tx_handle, db_handle);
}
