use db::DatabaseStore;
use std::collections::HashMap;
use tokio::sync::mpsc;

const MAX_CONVERSATION_RETRIES: u8 = 5;

/// A separate bounded MPSC worker fetches the session-named Redis stream, writes
/// its events into Neon, then removes the Redis stream only after that write commits.
pub async fn run_conversation_fetch_worker(
    mut requests: mpsc::Receiver<String>,
    transcripts: mpsc::Sender<(String, String)>,
    db: DatabaseStore,
) {
    let mut redis_connection = None;
    let mut retry_counts: HashMap<String, u8> = HashMap::new();
    while let Some(session_id) = requests.recv().await {
        if redis_connection.is_none() {
            redis_connection = match db.open_redis_poll_connection().await {
                Ok(connection) => Some(connection),
                Err(error) => {
                    eprintln!("Conversation Redis poll connection unavailable: {error}");
                    None
                }
            };
        }
        let persisted = match redis_connection.as_mut() {
            Some(connection) => {
                db.persist_conversation_from_redis_on(&session_id, connection)
                    .await
            }
            None => Err("Redis poll connection unavailable".to_string()),
        };
        // A failed poll connection is discarded and re-established for the next job.
        if persisted.is_err() {
            redis_connection = None;
        }
        let transcript = match persisted {
            Ok(Some(transcript)) => {
                let cleanup = db
                    .delete_redis_conversation_on(
                        &session_id,
                        redis_connection
                            .as_mut()
                            .expect("successful Redis connection"),
                    )
                    .await;
                if let Err(error) = cleanup {
                    eprintln!("Conversation persisted for {session_id}; Redis cleanup will retry: {error}");
                    requeue_or_dead_letter(&db, &mut retry_counts, &session_id).await;
                } else {
                    retry_counts.remove(&session_id);
                }
                if transcript.trim().is_empty() {
                    println!("Conversation events committed for {session_id}; no user/assistant transcript available for summarization");
                    continue;
                }
                transcript
            }
            Ok(None) => {
                // The summary queue can fire just after a previous worker has
                // committed and deleted the Redis stream. Reuse the durable
                // Neon transcript before treating this as a transient miss.
                match db.load_call_transcript(&session_id).await {
                    Ok(transcript) if !transcript.trim().is_empty() => {
                        println!("Conversation already persisted for {session_id}; using DB transcript for summarization");
                        retry_counts.remove(&session_id);
                        transcript
                    }
                    _ => {
                        eprintln!("Conversation persistence deferred for {session_id}: Redis stream is empty and no DB transcript exists");
                        requeue_or_dead_letter(&db, &mut retry_counts, &session_id).await;
                        continue;
                    }
                }
            }
            Err(error) => {
                eprintln!("Conversation persistence deferred for {session_id}: {error}");
                requeue_or_dead_letter(&db, &mut retry_counts, &session_id).await;
                continue;
            }
        };
        if transcripts.send((session_id, transcript)).await.is_err() {
            break;
        }
    }
}

async fn requeue_or_dead_letter(
    db: &DatabaseStore,
    retry_counts: &mut HashMap<String, u8>,
    session_id: &str,
) {
    let attempt = retry_counts.entry(session_id.to_string()).or_insert(0);
    *attempt = attempt.saturating_add(1);
    if *attempt > MAX_CONVERSATION_RETRIES {
        eprintln!("Conversation persistence dead-lettered for {session_id} after {MAX_CONVERSATION_RETRIES} retries");
        return;
    }
    let delay = 10_u64.saturating_mul(1_u64 << (*attempt as u32).min(5));
    if let Err(error) = db.enqueue_call_summary(session_id, delay).await {
        eprintln!("Could not requeue conversation fetch for {session_id}: {error}");
    } else {
        eprintln!("Conversation persistence retry {attempt}/{MAX_CONVERSATION_RETRIES} scheduled for {session_id} in {delay}s");
    }
}
