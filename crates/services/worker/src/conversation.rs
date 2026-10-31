use db::DatabaseStore;
use tokio::sync::mpsc;

/// A separate bounded MPSC worker fetches the session-named Redis stream, writes
/// its events into Neon, then removes the Redis stream only after that write commits.
pub async fn run_conversation_fetch_worker(
    mut requests: mpsc::Receiver<String>,
    transcripts: mpsc::Sender<(String, String)>,
    db: DatabaseStore,
) {
    let mut redis_connection = None;
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
            Some(connection) => db.persist_conversation_from_redis_on(&session_id, connection).await,
            None => Err("Redis poll connection unavailable".to_string()),
        };
        // A failed poll connection is discarded and re-established for the next job.
        if persisted.is_err() {
            redis_connection = None;
        }
        let transcript = match persisted {
            Ok(Some(transcript)) => {
                let cleanup = db.delete_redis_conversation_on(
                    &session_id,
                    redis_connection.as_mut().expect("successful Redis connection"),
                ).await;
                if let Err(error) = cleanup {
                    eprintln!("Conversation persisted for {session_id}; Redis cleanup will retry: {error}");
                    let _ = db.enqueue_call_summary(&session_id, 10).await;
                }
                transcript
            }
            Ok(None) | Err(_) => {
                eprintln!("Conversation persistence deferred for {session_id}");
                if let Err(error) = db.enqueue_call_summary(&session_id, 10).await {
                    eprintln!("Could not requeue conversation fetch: {error}");
                }
                continue;
            }
        };
        if transcripts.send((session_id, transcript)).await.is_err() { break; }
    }
}
