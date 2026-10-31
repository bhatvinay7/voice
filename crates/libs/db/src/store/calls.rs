use super::{DatabaseStore, DbResult, postgres::error};
use crate::models::{AdminGraphNode, CallEndpointsRecord, CallSummaryRecord, StreamEventRecord};
use chrono::Utc;
use redis::AsyncCommands;


impl DatabaseStore {
    pub fn generate_r2_vault_path(&self, agent_id: &str, category: &str, filename: &str) -> String {
        let now = Utc::now();
        let year = now.format("%Y").to_string();
        let month = now.format("%m").to_string();
        format!(
            "r2://{}/agents/{}/{}/{}/{}/{}",
            self.bucket_name, agent_id, category, year, month, filename
        )
    }

    pub async fn save_call_summary(
        &self,
        call_id: &str,
        user_id: &str,
        duration_seconds: u32,
        r2_text_presigned_url: &str,
        r2_audio_presigned_url: &str,
        summary: &str,
        budget: u64,
        location: &str,
    ) -> Result<CallSummaryRecord, String> {
        let record = CallSummaryRecord {
            call_id: call_id.to_string(),
            user_id: user_id.to_string(),
            duration_seconds,
            text_presigned_url: r2_text_presigned_url.to_string(),
            audio_presigned_url: r2_audio_presigned_url.to_string(),
            summary: summary.to_string(),
            qualified_budget_aed: budget,
            preferred_location: location.to_string(),
            created_at: Utc::now().to_rfc3339(),
        };

        self.save(&record).await?;
        Ok(record)
    }

    pub async fn save_call_endpoints(
        &self,
        session_id: &str,
        audio_url: &str,
        text_url: &str,
    ) -> Result<(), String> {
        let record = CallEndpointsRecord {
            session_id: session_id.to_string(),
            audio_url: audio_url.to_string(),
            text_url: text_url.to_string(),
            created_at: Utc::now().to_rfc3339(),
        };

        self.save(&record).await?;
        self.enqueue_call_summary(session_id, 5).await
    }

    pub async fn get_call_endpoints(&self, session_id: &str) -> DbResult<Option<CallEndpointsRecord>> { self.one(session_id).await }

    pub async fn get_all_call_endpoints(&self) -> DbResult<Vec<CallEndpointsRecord>> { self.all().await }

    pub async fn load_call_endpoints(&self) -> DbResult<Vec<CallEndpointsRecord>> {
        self.fetch("SELECT row_to_json(t)::text FROM public.call_endpoints t ORDER BY created_at DESC", &[]).await
    }

    pub async fn load_call_summaries(&self) -> DbResult<Vec<CallSummaryRecord>> {
        self.fetch("SELECT row_to_json(t)::text FROM public.call_summaries t ORDER BY created_at DESC", &[]).await
    }

    pub async fn load_call_summary(&self, call_id: &str) -> DbResult<Option<CallSummaryRecord>> { self.one(call_id).await }

    pub async fn enqueue_call_summary(
        &self,
        session_id: &str,
        delay_seconds: u64,
    ) -> Result<(), String> {
        let Some(client) = &self.redis_client else {
            return Err("Redis client is not available for summary jobs".to_string());
        };
        let mut connection = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|error| format!("Redis connection failed: {}", error))?;
        let queued: redis::RedisResult<()> = connection
            .zadd(
                "s3_sync_delay_queue",
                session_id,
                Utc::now().timestamp() + delay_seconds as i64,
            )
            .await;
        queued.map_err(|error| format!("Could not enqueue call summary: {}", error))
    }

    pub async fn load_call_transcript(&self, session_id: &str) -> DbResult<String> {
        let run_id = format!("run-{session_id}");
        let events: Vec<StreamEventRecord> = self.fetch("SELECT row_to_json(t)::text FROM public.stream_events t WHERE stream = 'stream:conversation' AND session_id IN ($1, $2) ORDER BY id", &[session_id, &run_id]).await?;
        let transcript = transcript_from_events(events.iter());
        if transcript.is_empty() { return Err(format!("No transcript events found for call {session_id}")); }
        Ok(transcript)
    }

    pub async fn load_recent_stream_events(&self) -> DbResult<Vec<StreamEventRecord>> {
        self.fetch("SELECT row_to_json(t)::text FROM public.stream_events t ORDER BY id DESC LIMIT 250", &[]).await
    }
    pub async fn load_session_events(&self, session_id: &str) -> DbResult<Vec<StreamEventRecord>> {
        self.fetch("SELECT row_to_json(t)::text FROM public.stream_events t WHERE session_id IN ($1, $2) ORDER BY id", &[session_id, &format!("run-{session_id}")]).await
    }

    pub async fn get_call_summaries(&self) -> DbResult<Vec<CallSummaryRecord>> { self.all().await }

    pub async fn get_call_summary(&self, call_id: &str) -> DbResult<Option<CallSummaryRecord>> { self.one(call_id).await }

    pub async fn get_graph_nodes(&self) -> DbResult<Vec<AdminGraphNode>> { self.all().await }

    pub async fn add_graph_node(&self, node: AdminGraphNode) -> DbResult<()> { self.save(&node).await }

    pub async fn update_graph_node(&self, node_id: &str, instruction: String) -> DbResult<bool> {
        Ok(sqlx::query("UPDATE public.admin_graph_nodes SET instruction = $2 WHERE id = $1").bind(node_id).bind(instruction).execute(&self.pool).await.map_err(error)?.rows_affected() > 0)
    }

    pub async fn delete_graph_node(&self, node_id: &str) -> DbResult<bool> { self.delete::<AdminGraphNode>(node_id).await }

    pub async fn push_redis_stream(
        &self,
        stream_name: &str,
        session_id: &str,
        trace_id: &str,
        event_type: &str,
        payload_json: &str,
    ) -> Result<String, String> {
        let record = StreamEventRecord {
            stream: stream_name.to_string(),
            session_id: session_id.to_string(),
            trace_id: trace_id.to_string(),
            event_type: event_type.to_string(),
            payload_json: payload_json.to_string(),
            timestamp: Utc::now().to_rfc3339(),
        };

        // Neon is the durable event/transcript source. Redis is only a transport.
        let id: i64 = sqlx::query_scalar("INSERT INTO public.stream_events (stream, session_id, trace_id, event_type, payload_json, timestamp) VALUES ($1, $2, $3, $4, $5, $6) RETURNING id")
            .bind(stream_name).bind(session_id).bind(trace_id).bind(event_type).bind(payload_json).bind(&record.timestamp)
            .fetch_one(&self.pool).await.map_err(error)?;
        Ok(id.to_string())
    }

    pub async fn set_json_with_ttl(
        &self,
        key: &str,
        payload_json: &str,
        ttl_seconds: usize,
    ) -> Result<(), String> {
        if let Some(ref client) = self.redis_client {
            if let Ok(mut con) = client.get_multiplexed_async_connection().await {
                return con.set_ex::<_, _, ()>(key, payload_json, ttl_seconds as u64).await.map_err(|_| "Redis write failed".into());
            }
        }
        Err("Redis client not available".to_string())
    }

    pub async fn get_json(&self, key: &str) -> Result<Option<String>, String> {
        if let Some(ref client) = self.redis_client {
            if let Ok(mut con) = client.get_multiplexed_async_connection().await {
                let res: Result<Option<String>, _> = con.get(key).await;
                return res.map_err(|e| format!("Redis error: {}", e));
            }
        }
        Ok(None)
    }
}

fn transcript_from_events<'a>(events: impl Iterator<Item = &'a StreamEventRecord>) -> String {
    transcript_from_payloads(
        events.map(|event| (event.event_type.clone(), event.payload_json.clone())),
    )
}

pub(super) fn transcript_from_payloads(events: impl Iterator<Item = (String, String)>) -> String {
    events
        .filter_map(|(event_type, payload)| {
            let payload: serde_json::Value = serde_json::from_str(&payload).ok()?;
            let (speaker, text) = match event_type.as_str() {
                "USER_PROMPT" => (
                    payload
                        .get("role")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("User"),
                    payload.get("text").and_then(serde_json::Value::as_str)?,
                ),
                "LLM_RESPONSE" => (
                    "Assistant",
                    payload
                        .get("response_text")
                        .and_then(serde_json::Value::as_str)?,
                ),
                _ => return None,
            };
            let text = text.trim();
            (!text.is_empty()).then(|| format!("{}: {}", speaker, text))
        })
        .collect::<Vec<_>>()
        .join("\n")
}
