use super::{postgres::error, DatabaseStore, DbResult};
use redis::{aio::MultiplexedConnection, streams::StreamRangeReply, FromRedisValue};
use std::{collections::HashSet, time::Duration};

#[derive(Debug)]
struct RedisConversationEvent {
    redis_entry_id: String,
    trace_id: String,
    event_type: String,
    payload_json: String,
    timestamp: String,
}

impl DatabaseStore {
    pub async fn open_redis_poll_connection(&self) -> DbResult<MultiplexedConnection> {
        let client = self.redis_client.as_ref().ok_or("Redis is not configured")?;
        tokio::time::timeout(Duration::from_secs(10), client.get_multiplexed_async_connection())
            .await
            .map_err(|_| "Redis connection timed out")?
            .map_err(|_| "Redis unavailable".to_string())
    }

    /// Appends to the stream named exactly after the call session. No TTL is set:
    /// the worker deletes this transient stream only after its Neon transaction commits.
    pub async fn append_conversation_to_redis(
        &self, session_id: &str, trace_id: &str, event_type: &str, payload_json: &str,
    ) -> DbResult<String> {
        if session_id.trim().is_empty() || session_id.starts_with("stream:") || session_id.starts_with("admin:") {
            return Err("Invalid conversation session name".into());
        }
        let timestamp = chrono::Utc::now().to_rfc3339();
        let mut connection = self.open_redis_poll_connection().await?;
        redis::cmd("XADD")
            .arg(session_id).arg("*")
            .arg("trace_id").arg(trace_id)
            .arg("event_type").arg(event_type)
            .arg("payload").arg(payload_json)
            .arg("timestamp").arg(&timestamp)
            .query_async(&mut connection).await
            .map_err(|_| "Redis conversation append failed".into())
    }

    /// Read-only live access for the agent; this never deletes the session stream.
    pub async fn load_redis_conversation(&self, session_id: &str) -> DbResult<Option<String>> {
        let mut connection = self.open_redis_poll_connection().await?;
        self.load_redis_conversation_on(session_id, &mut connection).await
    }

    pub async fn load_redis_conversation_on(
        &self, session_id: &str, connection: &mut MultiplexedConnection,
    ) -> DbResult<Option<String>> {
        let events = read_redis_conversation(session_id, connection).await?;
        let transcript = super::calls::transcript_from_payloads(
            events.into_iter().map(|event| (event.event_type, event.payload_json)),
        );
        Ok((!transcript.is_empty()).then_some(transcript))
    }

    /// The database consumer writes Redis events to Neon idempotently. The caller is
    /// responsible for deleting the Redis stream only after this method succeeds.
    pub async fn persist_conversation_from_redis_on(
        &self, session_id: &str, connection: &mut MultiplexedConnection,
    ) -> DbResult<Option<String>> {
        let events = read_redis_conversation(session_id, connection).await?;
        if events.is_empty() { return Ok(None); }
        let mut tx = self.pool.begin().await.map_err(error)?;
        for event in &events {
            sqlx::query("INSERT INTO public.stream_events (stream, session_id, trace_id, event_type, payload_json, timestamp, redis_entry_id) VALUES ('stream:conversation', $1, $2, $3, $4, $5, $6) ON CONFLICT DO NOTHING")
                .bind(session_id).bind(&event.trace_id).bind(&event.event_type)
                .bind(&event.payload_json).bind(&event.timestamp).bind(&event.redis_entry_id)
                .execute(&mut *tx).await.map_err(error)?;
        }
        tx.commit().await.map_err(error)?;
        let transcript = super::calls::transcript_from_payloads(
            events.into_iter().map(|event| (event.event_type, event.payload_json)),
        );
        Ok((!transcript.is_empty()).then_some(transcript))
    }

    pub async fn delete_redis_conversation_on(
        &self, session_id: &str, connection: &mut MultiplexedConnection,
    ) -> DbResult<()> {
        let _: i64 = redis::cmd("DEL").arg(session_id).query_async(connection).await
            .map_err(|_| "Redis conversation cleanup failed")?;
        Ok(())
    }
}

async fn read_redis_conversation(
    session_id: &str, connection: &mut MultiplexedConnection,
) -> DbResult<Vec<RedisConversationEvent>> {
    let mut start = "-".to_string();
    let mut seen = HashSet::new();
    let mut events = Vec::new();
    loop {
        let mut command = redis::cmd("XRANGE");
        command.arg(session_id).arg(&start).arg("+").arg("COUNT").arg(500);
        let page: StreamRangeReply = tokio::time::timeout(Duration::from_secs(10), command.query_async(connection))
            .await.map_err(|_| "Redis conversation read timed out")?
            .map_err(|_| "Redis conversation read failed")?;
        if page.ids.is_empty() { break; }
        for entry in &page.ids {
            if !seen.insert(entry.id.clone()) { continue; }
            let field = |name: &str| entry.map.get(name).and_then(|value| String::from_redis_value(value).ok());
            events.push(RedisConversationEvent {
                redis_entry_id: entry.id.clone(),
                trace_id: field("trace_id").ok_or("Conversation event missing trace_id")?,
                event_type: field("event_type").ok_or("Conversation event missing event_type")?,
                payload_json: field("payload").ok_or("Conversation event missing payload")?,
                timestamp: field("timestamp").ok_or("Conversation event missing timestamp")?,
            });
        }
        start = format!("({}", page.ids.last().expect("not empty").id);
    }
    Ok(events)
}
