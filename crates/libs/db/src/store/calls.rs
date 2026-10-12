use super::{this_lock, DatabaseStore};
use crate::models::{AdminGraphNode, CallEndpointsRecord, CallSummaryRecord, StreamEventRecord};
use chrono::Utc;
use redis::{streams::StreamRangeReply, AsyncCommands, FromRedisValue};
use uuid::Uuid;

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

        if let Some(client) = &self.redis_client {
            let mut connection = client
                .get_multiplexed_async_connection()
                .await
                .map_err(|error| format!("Redis connection failed: {}", error))?;
            let serialized = serde_json::to_string(&record)
                .map_err(|error| format!("Could not serialize call summary: {}", error))?;
            connection
                .hset::<_, _, _, ()>("admin:call_summaries", call_id, serialized)
                .await
                .map_err(|error| format!("Could not persist call summary: {}", error))?;
        }

        if let Ok(mut guard) = self.call_summaries.lock() {
            guard.retain(|saved| saved.call_id != call_id);
            guard.push(record.clone());
        }

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

        if let Some(client) = &self.redis_client {
            let mut connection = client
                .get_multiplexed_async_connection()
                .await
                .map_err(|error| format!("Redis connection failed: {}", error))?;
            let serialized = serde_json::to_string(&record)
                .map_err(|error| format!("Could not serialize recording record: {}", error))?;
            connection
                .hset::<_, _, _, ()>("admin:call_endpoints", session_id, serialized)
                .await
                .map_err(|error| format!("Could not persist recording record: {}", error))?;
            self.enqueue_call_summary(session_id, 5).await?;
        }
        let mut guard = this_lock(&self.call_endpoints)?;
        guard.retain(|saved| saved.session_id != session_id);
        guard.push(record);
        Ok(())
    }

    pub fn get_call_endpoints(&self, session_id: &str) -> Option<CallEndpointsRecord> {
        let guard = match this_lock(&self.call_endpoints) {
            Ok(g) => g,
            Err(_) => return None,
        };
        guard.iter().find(|r| r.session_id == session_id).cloned()
    }

    pub fn get_all_call_endpoints(&self) -> Vec<CallEndpointsRecord> {
        let guard = match this_lock(&self.call_endpoints) {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        guard.clone()
    }

    pub async fn load_call_endpoints(&self) -> Result<Vec<CallEndpointsRecord>, String> {
        let Some(client) = &self.redis_client else {
            return Ok(self.get_all_call_endpoints());
        };
        let mut connection = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|error| format!("Redis connection failed: {}", error))?;
        let saved: std::collections::HashMap<String, String> = connection
            .hgetall("admin:call_endpoints")
            .await
            .map_err(|error| format!("Could not load call recordings: {}", error))?;
        let mut records = saved
            .into_values()
            .map(|value| {
                serde_json::from_str::<CallEndpointsRecord>(&value)
                    .map_err(|error| format!("Could not decode saved recording record: {}", error))
            })
            .collect::<Result<Vec<_>, _>>()?;
        records.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        Ok(records)
    }

    pub async fn load_call_summaries(&self) -> Result<Vec<CallSummaryRecord>, String> {
        let Some(client) = &self.redis_client else {
            return Ok(self.get_call_summaries());
        };
        let mut connection = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|error| format!("Redis connection failed: {}", error))?;
        let saved: std::collections::HashMap<String, String> = connection
            .hgetall("admin:call_summaries")
            .await
            .map_err(|error| format!("Could not load call summaries: {}", error))?;
        let mut records = saved
            .into_values()
            .map(|value| {
                serde_json::from_str::<CallSummaryRecord>(&value)
                    .map_err(|error| format!("Could not decode saved call summary: {}", error))
            })
            .collect::<Result<Vec<_>, _>>()?;
        records.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        Ok(records)
    }

    pub async fn load_call_summary(
        &self,
        call_id: &str,
    ) -> Result<Option<CallSummaryRecord>, String> {
        let Some(client) = &self.redis_client else {
            return Ok(self.get_call_summary(call_id));
        };
        let mut connection = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|error| format!("Redis connection failed: {}", error))?;
        let saved: Option<String> = connection
            .hget("admin:call_summaries", call_id)
            .await
            .map_err(|error| format!("Could not load call summary: {}", error))?;
        saved
            .map(|value| {
                serde_json::from_str::<CallSummaryRecord>(&value)
                    .map_err(|error| format!("Could not decode saved call summary: {}", error))
            })
            .transpose()
    }

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

    pub async fn load_call_transcript(&self, session_id: &str) -> Result<String, String> {
        let run_session_id = format!("run-{}", session_id);
        let Some(client) = &self.redis_client else {
            let events = self
                .stream_events
                .lock()
                .map_err(|error| format!("Could not read conversation events: {}", error))?;
            return Ok(transcript_from_events(events.iter().filter(|event| {
                event.stream == "stream:conversation"
                    && (event.session_id == session_id || event.session_id == run_session_id)
            })));
        };

        let mut connection = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|error| format!("Redis connection failed: {}", error))?;
        let session_stream = format!("stream:conversation:{}", session_id);
        let mut reply: StreamRangeReply = redis::cmd("XRANGE")
            .arg(&session_stream)
            .arg("-")
            .arg("+")
            .query_async(&mut connection)
            .await
            .map_err(|error| format!("Could not read call transcript: {}", error))?;

        // Agent conversation writes historically used `run-{call_id}`, while summary jobs
        // are keyed by the unprefixed call ID. Read that per-session stream as a compatibility
        // fallback so existing calls can still be summarized.
        if reply.ids.is_empty() {
            let run_session_stream = format!("stream:conversation:{}", run_session_id);
            reply = redis::cmd("XRANGE")
                .arg(&run_session_stream)
                .arg("-")
                .arg("+")
                .query_async(&mut connection)
                .await
                .map_err(|error| format!("Could not read call transcript: {}", error))?;
        }

        // Older calls predate the per-session stream index. Fall back to recent global events.
        if reply.ids.is_empty() {
            reply = redis::cmd("XREVRANGE")
                .arg("stream:conversation")
                .arg("+")
                .arg("-")
                .arg("COUNT")
                .arg(10000)
                .query_async(&mut connection)
                .await
                .map_err(|error| format!("Could not read call transcript: {}", error))?;
            reply.ids.reverse();
        }

        let events = reply.ids.iter().filter_map(|entry| {
            let field = |name: &str| {
                entry
                    .map
                    .get(name)
                    .and_then(|value| String::from_redis_value(value).ok())
            };
            let (Some(event_session), Some(event_type), Some(payload_json)) =
                (field("session_id"), field("event_type"), field("payload"))
            else {
                return None;
            };
            if event_session != session_id && event_session != run_session_id {
                return None;
            }
            Some((event_type, payload_json))
        });
        let transcript = transcript_from_payloads(events);
        if transcript.is_empty() {
            return Err(format!(
                "No transcript events found for call {}",
                session_id
            ));
        }
        Ok(transcript)
    }

    pub async fn load_recent_stream_events(&self) -> Result<Vec<StreamEventRecord>, String> {
        let Some(client) = &self.redis_client else {
            return self
                .stream_events
                .lock()
                .map(|events| events.clone())
                .map_err(|error| format!("Could not read in-memory stream events: {}", error));
        };
        let mut connection = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|error| format!("Redis connection failed: {}", error))?;
        let mut records = Vec::new();
        for stream in ["stream:audit", "stream:conversation", "stream:logs"] {
            let reply: StreamRangeReply = redis::cmd("XREVRANGE")
                .arg(stream)
                .arg("+")
                .arg("-")
                .arg("COUNT")
                .arg(250)
                .query_async(&mut connection)
                .await
                .map_err(|error| format!("Could not read {}: {}", stream, error))?;
            for entry in reply.ids {
                let value = |field: &str| -> Option<String> {
                    entry
                        .map
                        .get(field)
                        .and_then(|value| String::from_redis_value(value).ok())
                };
                let (
                    Some(session_id),
                    Some(trace_id),
                    Some(event_type),
                    Some(payload_json),
                    Some(timestamp),
                ) = (
                    value("session_id"),
                    value("trace_id"),
                    value("event_type"),
                    value("payload"),
                    value("timestamp"),
                )
                else {
                    continue;
                };
                records.push(StreamEventRecord {
                    stream: stream.to_string(),
                    session_id,
                    trace_id,
                    event_type,
                    payload_json,
                    timestamp,
                });
            }
        }
        records.sort_by(|left, right| right.timestamp.cmp(&left.timestamp));
        records.truncate(250);
        Ok(records)
    }

    pub fn get_call_summaries(&self) -> Vec<CallSummaryRecord> {
        let guard = match this_lock(&self.call_summaries) {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        guard.clone()
    }

    pub fn get_call_summary(&self, call_id: &str) -> Option<CallSummaryRecord> {
        let guard = match this_lock(&self.call_summaries) {
            Ok(g) => g,
            Err(_) => return None,
        };
        guard.iter().find(|r| r.call_id == call_id).cloned()
    }

    pub fn get_graph_nodes(&self) -> Vec<AdminGraphNode> {
        let guard = match this_lock(&self.graph_nodes) {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        guard.clone()
    }

    pub fn add_graph_node(&self, node: AdminGraphNode) {
        if let Ok(mut guard) = self.graph_nodes.lock() {
            guard.push(node);
        }
    }

    pub fn update_graph_node(&self, node_id: &str, new_instruction: String) -> bool {
        if let Ok(mut guard) = self.graph_nodes.lock() {
            if let Some(node) = guard.iter_mut().find(|n| n.id == node_id) {
                node.instruction = new_instruction;
                return true;
            }
        }
        false
    }

    pub fn delete_graph_node(&self, node_id: &str) -> bool {
        if let Ok(mut guard) = self.graph_nodes.lock() {
            let initial_len = guard.len();
            guard.retain(|n| n.id != node_id);
            return guard.len() < initial_len;
        }
        false
    }

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

        let Some(client) = &self.redis_client else {
            return Err("Redis client is not available for event persistence".to_string());
        };
        let mut connection = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|error| format!("Redis connection failed: {}", error))?;
        let fields = [
            ("session_id", session_id),
            ("trace_id", trace_id),
            ("event_type", event_type),
            ("payload", payload_json),
            ("timestamp", &record.timestamp),
        ];
        let _stream_id: String = connection
            .xadd(stream_name, "*", &fields)
            .await
            .map_err(|error| format!("Could not persist event in {}: {}", stream_name, error))?;
        if stream_name == "stream:conversation" {
            let session_stream = format!("{}:{}", stream_name, session_id);
            let _: String = connection
                .xadd(&session_stream, "*", &fields)
                .await
                .map_err(|error| {
                    format!(
                        "Could not index conversation event for {}: {}",
                        session_id, error
                    )
                })?;
        }
        if let Ok(mut guard) = self.stream_events.lock() {
            guard.push(record);
        }

        Ok(format!("stream-id-{}", Uuid::new_v4()))
    }

    pub async fn set_json_with_ttl(
        &self,
        key: &str,
        payload_json: &str,
        ttl_seconds: usize,
    ) -> Result<(), String> {
        if let Some(ref client) = self.redis_client {
            if let Ok(mut con) = client.get_multiplexed_async_connection().await {
                let _res: Result<(), _> = con.set_ex(key, payload_json, ttl_seconds as u64).await;
                return Ok(());
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

fn transcript_from_payloads(events: impl Iterator<Item = (String, String)>) -> String {
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
