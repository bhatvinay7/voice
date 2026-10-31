//! One-time import for installations that previously kept call history in Redis.
use super::{DatabaseStore, DbResult, postgres::{Record, error}};
use crate::{CallEndpointsRecord, CallSummaryRecord};
use redis::{AsyncCommands, FromRedisValue, streams::StreamRangeReply};

impl DatabaseStore {
    pub async fn import_legacy_redis(&self) -> DbResult<(usize, usize, usize)> {
        let client = self.redis_client.as_ref().ok_or("Redis is not configured")?;
        let mut redis = client.get_multiplexed_async_connection().await.map_err(|_| "Cannot connect to legacy Redis")?;
        let summaries: Vec<String> = redis.hvals("admin:call_summaries").await.map_err(|_| "Cannot read legacy summaries")?;
        let endpoints: Vec<String> = redis.hvals("admin:call_endpoints").await.map_err(|_| "Cannot read legacy recordings")?;
        let mut counts = (0, 0, 0);
        for value in summaries {
            let record: CallSummaryRecord = serde_json::from_str(&value).map_err(|_| "Invalid legacy summary")?;
            counts.0 += self.import_record(&record).await?;
        }
        for value in endpoints {
            let record: CallEndpointsRecord = serde_json::from_str(&value).map_err(|_| "Invalid legacy recording")?;
            counts.1 += self.import_record(&record).await?;
        }
        for stream in ["stream:audit", "stream:conversation", "stream:logs"] {
            let mut start = "-".to_string();
            loop {
                let page: StreamRangeReply = redis::cmd("XRANGE").arg(stream).arg(&start).arg("+").arg("COUNT").arg(500)
                    .query_async(&mut redis).await.map_err(|_| "Cannot read legacy events")?;
                if page.ids.is_empty() { break; }
                for event in &page.ids {
                    let field = |key: &str| -> DbResult<String> {
                        event.map.get(key).and_then(|v| String::from_redis_value(v).ok()).ok_or_else(|| format!("Legacy event missing {key}"))
                    };
                    let mut tx = self.pool.begin().await.map_err(error)?;
                    let inserted = sqlx::query("INSERT INTO public.actor_idempotency (key, response) VALUES ($1, ''::bytea) ON CONFLICT DO NOTHING")
                        .bind(format!("legacy-import:{stream}:{}", event.id)).execute(&mut *tx).await.map_err(error)?.rows_affected();
                    if inserted > 0 {
                        sqlx::query("INSERT INTO public.stream_events (stream, session_id, trace_id, event_type, payload_json, timestamp) VALUES ($1,$2,$3,$4,$5,$6)")
                            .bind(stream).bind(field("session_id")?).bind(field("trace_id")?).bind(field("event_type")?).bind(field("payload")?).bind(field("timestamp")?)
                            .execute(&mut *tx).await.map_err(error)?;
                        counts.2 += 1;
                    }
                    tx.commit().await.map_err(error)?;
                }
                start = format!("({}", page.ids.last().unwrap().id);
            }
        }
        Ok(counts)
    }

    async fn import_record<T: Record>(&self, record: &T) -> DbResult<usize> {
        let insert = format!("{} DO NOTHING", T::UPSERT.split(" DO UPDATE SET ").next().unwrap());
        Ok(sqlx::query(&insert).bind(serde_json::to_string(record).map_err(|_| "Invalid legacy record")?)
            .execute(&self.pool).await.map_err(error)?.rows_affected() as usize)
    }
}
