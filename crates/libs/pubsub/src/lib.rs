use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SystemEvent {
    CallSessionStarted {
        call_id: String,
        caller_id: String,
        language: String,
    },
    CallSessionEnded {
        call_id: String,
        duration_ms: u64,
        summary: String,
    },
    VoiceSttTranscript {
        call_id: String,
        text: String,
        is_final: bool,
        language: String,
    },
    VoiceTtsStream {
        call_id: String,
        audio_chunk_base64: String,
        index: u32,
        is_end: bool,
    },
    VoiceBargeIn {
        call_id: String,
        timestamp_ms: u64,
    },
    AgentToolExecute {
        call_id: String,
        tool_name: String,
        args: serde_json::Value,
        idempotency_key: String,
    },
    AgentToolResponse {
        call_id: String,
        tool_name: String,
        result: serde_json::Value,
        idempotency_key: String,
    },
    AuditEvent {
        call_id: Option<String>,
        event_type: String,
        payload: serde_json::Value,
        timestamp: String,
    },
}

#[derive(Clone)]
pub struct PubSubBus {
    sender: broadcast::Sender<SystemEvent>,
}

impl PubSubBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    pub fn publish(&self, event: SystemEvent) -> Result<usize, String> {
        if event.call_id().is_some_and(|id| id.starts_with("phone:")) {
            return Ok(0);
        }
        self.sender
            .send(event)
            .map_err(|e| format!("Failed to publish system event: {}", e))
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SystemEvent> {
        this_receiver(&self.sender)
    }
}

fn this_receiver(sender: &broadcast::Sender<SystemEvent>) -> broadcast::Receiver<SystemEvent> {
    sender.subscribe()
}

pub type SharedPubSubBus = Arc<PubSubBus>;

pub fn create_shared_bus(capacity: usize) -> SharedPubSubBus {
    Arc::new(PubSubBus::new(capacity))
}

impl SystemEvent {
    pub fn call_id(&self) -> Option<&str> {
        match self {
            Self::CallSessionStarted { call_id, .. }
            | Self::CallSessionEnded { call_id, .. }
            | Self::VoiceSttTranscript { call_id, .. }
            | Self::VoiceTtsStream { call_id, .. }
            | Self::VoiceBargeIn { call_id, .. }
            | Self::AgentToolExecute { call_id, .. }
            | Self::AgentToolResponse { call_id, .. } => Some(call_id),
            Self::AuditEvent { call_id, .. } => call_id.as_deref(),
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/pubsub_pubsub_lib_phone_isolation_tests_1.rs"]
mod phone_isolation_tests;
