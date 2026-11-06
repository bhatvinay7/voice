use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

pub fn elapsed_audio_samples(recording_started_at: Instant) -> u64 {
    let samples = recording_started_at
        .elapsed()
        .as_nanos()
        .saturating_mul(16_000)
        / 1_000_000_000;
    u64::try_from(samples).unwrap_or(u64::MAX)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunId(pub String);

impl RunId {
    /// Return the canonical call/session identifier used by persistence and APIs.
    /// The `run-` prefix is retained only for internal actor run IDs.
    pub fn call_id(&self) -> &str {
        self.0.strip_prefix("run-").unwrap_or(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SpeakerRole {
    Candidate,
    Interviewer { interviewer_id: String },
    System,
}

#[derive(Debug, Clone)]
pub enum QueueEvent {
    SessionStarted {
        session_id: String,
        user_id: String,
        language: String,
    },
    SessionResumed {
        session_id: String,
    },
    SessionCancelled {
        session_id: String,
    },
    UserAudioChunk {
        session_id: String,
        pcm_base64: String,
        is_final: bool,
    },
    /// Decoded phone-provider audio captured for the normal recording pipeline.
    TelephonyAudio {
        session_id: String,
        samples: Vec<i16>,
        sample_rate: u32,
        channels: u16,
        elapsed_samples: u64,
    },
    SttTranscript {
        session_id: String,
        text: String,
        language: String,
        is_final: bool,
        /// Noise profile from the VAD analysis window for this utterance.
        noise: Option<crate::audio_gate::NoiseProfile>,
    },
    /// Ordered/revision-aware transcript input for the reactive session graph.
    TranscriptFragment {
        session_id: String,
        stream_id: String,
        sequence_id: u64,
        text: String,
        is_final: bool,
    },
    InterviewerSpeech {
        session_id: String,
        interviewer_id: String,
        active: bool,
    },
    BargeInTrigger {
        session_id: String,
        timestamp_ms: u64,
    },
    Tool(ToolEvent),
    Agent(AgentEvent),
    StsEvent {
        session_id: String,
        event: StsPipelineEvent,
    },
    GraphNodesFetched {
        session_id: String,
        user_id: String,
        language: String,
        task_id: TaskId,
        nodes: Vec<db::AdminGraphNode>,
    },
    DbActionCompleted {
        task_id: TaskId,
        action: String,
        success: bool,
        error: Option<String>,
    },
    /// Demand-only graph access. The request itself contains no graph data;
    /// the owning session constructs a query-scoped component only here.
    GraphContextRequested {
        session_id: String,
        query_id: String,
        reply_to: mpsc::Sender<GraphContextResponse>,
    },
    /// A background graph worker may propose enrichment, but it must return
    /// through the owning SessionActor.  The actor validates the immutable
    /// query ID and base revision before it mutates its in-memory graph.
    GraphEnrichmentReady {
        session_id: String,
        query_id: String,
        base_graph_version: u64,
        enrichment: String,
    },
    /// Lightweight observability request. The reply is constructed by the
    /// owning session actor; callers never read its graph or task map directly.
    SessionStateRequested {
        session_id: String,
        reply_to: mpsc::Sender<SessionStateSnapshot>,
    },
    CommitTranscript {
        session_id: String,
        query_id: String,
    },
    /// Internal durable-scheduler wake-up. Payload lives in Sled, not MPSC.
    ScheduledTaskDue {
        session_id: String,
        task_id: TaskId,
    },
}

#[derive(Debug, Clone)]
pub struct GraphContextResponse {
    pub session_id: String,
    pub query_id: String,
    pub graph_version: u64,
    pub context: Option<String>,
}

/// Copy-only session diagnostics returned through the session mailbox.
/// It deliberately contains no graph nodes, transcript text, tool arguments,
/// or channel handles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionStateSnapshot {
    pub session_id: String,
    pub graph_version: u64,
    pub active_task_count: usize,
    pub running_task_count: usize,
    pub retrying_task_count: usize,
    pub pending_transcript: bool,
    pub sts_idle: bool,
    pub speech_generation_id: u64,
}

pub type SessionEvent = QueueEvent;

#[derive(Debug, Clone)]
pub enum PendingDbAction {
    GetGraphNodes {
        session_id: String,
        user_id: String,
        language: String,
    },
    PushRedisStream {
        stream_name: String,
        session_id: String,
        trace_id: String,
        event_type: String,
        payload_json: String,
    },
    SetJsonWithTtl {
        key: String,
        payload_json: String,
        ttl_seconds: usize,
    },
    SaveS3Endpoints {
        session_id: String,
        audio_url: String,
        text_url: String,
    },
}

#[derive(Debug, Clone)]
pub enum StsPipelineCmd {
    /// Legacy generic ingress. New production call sites must use the
    /// source-specific fast or agent channels below.
    InjectContext { text: String },
    /// Fast planner speech: clarification or a short tool-wait acknowledgement.
    /// This has priority and revokes queued main-agent audio.
    InjectFastContext { text: String },
    /// Main agent speech: verified graph-context or post-tool answer.
    InjectAgentContext { text: String },
    /// Decision Engine interrupts the STS model immediately (Barge-in / Cancel)
    Stop,
    /// Ends the per-session LiveKit and audio workers.
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum StsPipelineEvent {
    /// STS Model finishes outputting the current response
    AudioComplete,
    /// Real-time transcript chunk of what the user is saying natively heard by STS
    UserTranscriptChunk {
        text: String,
        is_final: bool,
        /// Noise profile attached by the VAD gate (None if no profile is available).
        noise: Option<crate::audio_gate::NoiseProfile>,
    },
    /// Real-time transcript of what the STS model is currently speaking
    AiTranscriptChunk { text: String },
    /// Caller speech has started. Stop local playback immediately even if the
    /// transport's playback flag has already fallen behind buffered audio.
    UserSpeechStarted,
    /// User interrupted the AI
    BargeIn,
    /// Non-fatal or fatal error inside the STS pipeline
    Error(String),
}

/// Raw WebRTC audio bytes generated by the STS model to be sent directly to the client.
#[derive(Debug, Clone)]
pub struct WebRtcAudioChunk {
    pub session_id: String,
    pub pcm_bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
pub enum AudioSpeaker {
    User,
    Assistant,
}

#[derive(Debug, Clone)]
pub struct TtsAudioPacket {
    pub session_id: String,
    pub samples: Vec<i16>,
    pub sample_rate: u32,
    pub channels: u16,
    pub elapsed_samples: u64,
    pub speaker: AudioSpeaker,
}

pub enum S3DumpData {
    Text {
        session_id: String,
        payload: String,
    },
    Audio {
        session_id: String,
        samples: Vec<i16>,
        sample_rate: u32,
        channels: u16,
        elapsed_samples: u64,
        speaker: AudioSpeaker,
    },
    Finalize {
        session_id: String,
        reply_to: tokio::sync::oneshot::Sender<Result<(String, String), String>>,
    },
}

/// Marks whether a conversation turn should be persisted to the AI's live
/// Redis context window.
///
/// | Value        | Redis (AI history) | S3 (audit log) |
/// |--------------|--------------------|----------------|
/// | `Relevant`   | ✓ stored           | ✓ stored       |
/// | `Irrelevant` | ✗ skipped          | ✓ stored       |
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversationRelevance {
    /// On-domain exchange — feed to AI on subsequent turns.
    Relevant,
    /// Off-domain exchange (no results / fallback) — drop from AI history.
    Irrelevant,
}

impl Default for ConversationRelevance {
    fn default() -> Self {
        Self::Relevant
    }
}

impl ConversationRelevance {
    pub fn is_relevant(self) -> bool {
        self == Self::Relevant
    }
}

/// Live transcript event fanned out inside one call session.
/// S3 and Redis each subscribe to the same per-session broadcast channel;
/// only Redis respects the relevance tag.
#[derive(Debug, Clone)]
pub struct ConversationEvent {
    pub session_id: String,
    pub trace_id: String,
    pub event_type: String,
    pub payload_json: String,
    /// Whether this turn should be written into the AI's Redis context window.
    /// Defaults to `Relevant`. Set to `Irrelevant` for off-domain fallback turns
    /// so they don't pollute the conversation history fed to the LLM.
    pub relevance: ConversationRelevance,
}

#[derive(Debug, Clone)]
pub enum AgentCmd {
    /// Installs the session-scoped conversation broadcast channel once the actor starts.
    SetConversationFanout {
        run_id: RunId,
        conversation_tx: tokio::sync::broadcast::Sender<ConversationEvent>,
    },
    Start {
        run_id: RunId,
        input: String,
    },
    UserInput {
        run_id: RunId,
        user_id: String,
        role: SpeakerRole,
        text: String,
        /// Sanitised (noise-stripped) text sent to the LLM; equals `text` when clean.
        clean_text: String,
        /// Noise profile from the VAD window, if available.
        noise: Option<crate::audio_gate::NoiseProfile>,
    },
    /// Associates the next LLM decision with the immutable graph query ID.
    SetGraphQuery {
        run_id: RunId,
        query_id: String,
        /// Playback generation observed when this immutable user turn opened.
        playback_generation: u64,
    },
    /// The user asked to continue or repeat the previous answer.
    ContinueLast {
        run_id: RunId,
        kind: crate::speech_filter::ContinuationKind,
    },
    InjectToneContext {
        run_id: RunId,
        session_id: String,
        tone_intent: String,
        confidence: f32,
    },
    SetAiListenMute {
        run_id: RunId,
        muted: bool,
    },
    ToolHeartbeat {
        run_id: RunId,
        task_id: TaskId,
        progress_pct: u8,
    },
    ToolRetrying {
        run_id: RunId,
        task_id: TaskId,
        attempt: u32,
    },
    ToolResult {
        run_id: RunId,
        task_id: TaskId,
        query_id: Option<String>,
        outcome: Result<serde_json::Value, String>,
        /// Immutable question and branch context captured by the owning
        /// session actor after the tool result was merged into its graph.
        user_question: Option<String>,
        graph_context: Option<String>,
    },
    /// Abort in-flight generation for a spoken turn. This is non-terminal:
    /// the same session may immediately receive a new turn.
    CancelGeneration {
        run_id: RunId,
    },
    Cancel {
        run_id: RunId,
    },
}

#[derive(Debug, Clone)]
pub enum AgentEvent {
    ToolRequested {
        run_id: RunId,
        task_id: TaskId,
        query_id: Option<String>,
        playback_generation: u64,
        /// Other graph query results that must be terminal before this tool's
        /// caller-facing answer may be generated.
        depends_on_query_ids: Vec<String>,
        tool_name: String,
        /// Brief acknowledgement generated by the fast turn planner for this
        /// specific retrieval. It is spoken only when a tool is actually run.
        wait_message: Option<String>,
        args: serde_json::Value,
    },
    NeedsUserInput {
        run_id: RunId,
        question: String,
    },
    /// Fast planner determined this turn depends on a pending tool query.
    /// The SessionActor grants the source task a one-time answer permit for
    /// this newer query; the tool itself remains owned by its original turn.
    WaitForRelatedTool {
        run_id: RunId,
        query_id: String,
        depends_on_query_id: String,
        wait_message: Option<String>,
    },
    Finished {
        run_id: RunId,
        /// Immutable graph query ID that produced this response. The session
        /// uses it to reject output from an older user turn.
        query_id: Option<String>,
        playback_generation: u64,
        result: String,
        /// Whether this response was domain-relevant.
        /// `false` means the agent replied with a domain-fallback (e.g. "no properties found");
        /// the session actor should suppress both this AI turn AND the preceding user turn
        /// from the Redis AI-history context window.
        relevance: ConversationRelevance,
    },
    Failed {
        run_id: RunId,
        error: String,
    },
}

#[derive(Debug, Clone)]
pub enum ToolCmd {
    Execute {
        task_id: TaskId,
        attempt: u32,
        tool_name: String,
        args: serde_json::Value,
    },
    Cancel {
        task_id: TaskId,
    },
}

#[derive(Debug, Clone)]
pub enum ToolEvent {
    Heartbeat {
        task_id: TaskId,
        attempt: u32,
        query_id: Option<String>,
        playback_generation: u64,
        progress_pct: u8,
    },
    Done {
        task_id: TaskId,
        attempt: u32,
        query_id: Option<String>,
        playback_generation: u64,
        output: serde_json::Value,
    },
    Failed {
        task_id: TaskId,
        attempt: u32,
        query_id: Option<String>,
        playback_generation: u64,
        error: String,
        retryable: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    Running,
    Retrying,
    Succeeded,
    Failed,
    TimedOut,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct TaskStateRecord {
    pub task_id: TaskId,
    pub run_id: RunId,
    pub tool_name: String,
    pub args: serde_json::Value,
    /// Graph node and revision that requested this tool. Tool completion must
    /// merge into this branch, never into a newer caller turn.
    pub query_node_id: Option<String>,
    /// Immutable user-query event identity copied through tool execution.
    pub query_id: Option<String>,
    pub query_graph_version: u64,
    /// Playback generation that owned the request at dispatch time.
    pub playback_generation: u64,
    /// Newer query IDs permitted to consume this tool result after the fast
    /// planner explicitly established a relationship.
    pub dependent_query_ids: Vec<String>,
    /// Source queries selected by the fast planner as prerequisites for this
    /// query's main-agent answer gate.
    pub required_query_ids: Vec<String>,
    /// Verified result retained while the answer gate waits for prerequisites.
    pub completed_output: Option<serde_json::Value>,
    pub answer_released: bool,
    pub status: TaskStatus,
    pub attempt: u32,
    pub max_retry: u32,
    pub retry_count: u32,
    pub timeout_duration: Duration,
    pub error: Option<String>,
}
