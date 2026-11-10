use db::{AdminGraphNode, DatabaseStore, DbResult};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::collections::{BTreeSet, HashMap};
use std::time::{Duration, Instant};

/// Durable graph-definition operations. This owns the admin graph boundary;
/// session runs only receive a copy of the definition.
#[derive(Clone)]
pub struct WorkflowRepository {
    store: DatabaseStore,
}

impl WorkflowRepository {
    pub fn new(store: DatabaseStore) -> Self {
        Self { store }
    }

    pub async fn fetch(&self) -> DbResult<Vec<AdminGraphNode>> {
        self.store.get_graph_nodes().await
    }

    pub async fn create(&self, node: AdminGraphNode) -> DbResult<()> {
        self.store.add_graph_node(node).await
    }

    pub async fn update_instruction(&self, node_id: &str, instruction: &str) -> DbResult<bool> {
        self.store
            .update_graph_node(node_id, instruction.to_string())
            .await
    }

    pub async fn delete(&self, node_id: &str) -> DbResult<bool> {
        self.store.delete_graph_node(node_id).await
    }

    pub async fn sync_to_graph_database(&self, graph: &db::neo4rs::Graph) -> DbResult<usize> {
        let nodes = self.fetch().await?;
        db::push_workflow_dag_to_graph(graph, &nodes).await?;
        Ok(nodes.len())
    }
}

/// An in-memory graph run owned by exactly one call session. Releasing this
/// run never changes the durable admin definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionWorkflow {
    pub run_id: String,
    pub active_node_id: Option<String>,
    pub nodes: BTreeMap<String, SessionWorkflowNode>,
    pub status: SessionWorkflowStatus,
    pub graph_version: u64,
    pub generation_epoch: u64,
    pub nodes_by_dependency: HashMap<String, BTreeSet<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionWorkflowNode {
    pub id: String,
    pub step_type: String,
    pub instruction: String,
    pub status: String,
    pub turn_id: u64,
    pub revision: u64,
    pub parent_ids: Vec<String>,
    pub input_sequence: u64,
    #[serde(skip)]
    pub deadline: Option<Instant>,
    pub tool_call_id: Option<String>,
    pub generation_id: Option<u64>,
    pub missing_dependencies: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionWorkflowStatus {
    Active,
    Completed,
    Failed,
    Released,
}

impl SessionWorkflow {
    pub fn create(run_id: impl Into<String>, definition: Vec<AdminGraphNode>) -> Self {
        let mut nodes = BTreeMap::new();
        let mut active_node_id = None;
        for node in definition {
            if active_node_id.is_none() {
                active_node_id = Some(node.id.clone());
            }
            nodes.insert(
                node.id.clone(),
                SessionWorkflowNode {
                    id: node.id,
                    step_type: node.step_type,
                    instruction: node.instruction,
                    status: "Pending".to_string(),
                    turn_id: 0,
                    revision: 0,
                    parent_ids: Vec::new(),
                    input_sequence: 0,
                    deadline: None,
                    tool_call_id: None,
                    generation_id: None,
                    missing_dependencies: Vec::new(),
                },
            );
        }
        Self {
            run_id: run_id.into(),
            active_node_id,
            nodes,
            status: SessionWorkflowStatus::Active,
            graph_version: 0,
            generation_epoch: 0,
            nodes_by_dependency: HashMap::new(),
        }
    }

    pub fn revise(&mut self, node_id: &str, content: impl Into<String>, revision: u64) -> bool {
        let Some(node) = self.nodes.get_mut(node_id) else {
            return false;
        };
        node.instruction = content.into();
        node.revision = revision;
        self.graph_version = self.graph_version.saturating_add(1);
        true
    }

    pub fn mark_ready(&mut self, node_id: &str, missing: Vec<String>) -> bool {
        let Some(node) = self.nodes.get_mut(node_id) else {
            return false;
        };
        node.missing_dependencies = missing;
        node.status = if node.missing_dependencies.is_empty() {
            "ReadyForGeneration"
        } else {
            "WaitingForContext"
        }
        .to_string();
        self.graph_version = self.graph_version.saturating_add(1);
        true
    }

    pub fn invalidate_generation(&mut self, node_id: &str) -> bool {
        let Some(node) = self.nodes.get_mut(node_id) else {
            return false;
        };
        node.generation_id = None;
        node.status = "Superseded".to_string();
        self.generation_epoch = self.generation_epoch.saturating_add(1);
        self.graph_version = self.graph_version.saturating_add(1);
        true
    }

    pub fn complete(&mut self) {
        self.status = SessionWorkflowStatus::Completed;
    }

    pub fn fail(&mut self) {
        self.status = SessionWorkflowStatus::Failed;
    }

    pub fn release(&mut self) {
        self.nodes.clear();
        self.active_node_id = None;
        self.status = SessionWorkflowStatus::Released;
        self.graph_version = self.graph_version.saturating_add(1);
        self.generation_epoch = self.generation_epoch.saturating_add(1);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptFragment {
    pub stream_id: String,
    pub sequence_id: u64,
    pub text: String,
    pub is_final: bool,
}

#[derive(Debug)]
pub struct ReorderBuffer {
    next_sequence: u64,
    pending: BTreeMap<u64, TranscriptFragment>,
    seen: BTreeSet<u64>,
    gap_started: Option<Instant>,
    max_gap_wait: Duration,
}

impl ReorderBuffer {
    pub fn new(start_sequence: u64, max_gap_wait: Duration) -> Self {
        Self {
            next_sequence: start_sequence,
            pending: BTreeMap::new(),
            seen: BTreeSet::new(),
            gap_started: None,
            max_gap_wait,
        }
    }
    pub fn push(&mut self, fragment: TranscriptFragment, now: Instant) -> Vec<TranscriptFragment> {
        if !self.seen.insert(fragment.sequence_id) || fragment.sequence_id < self.next_sequence {
            return Vec::new();
        }
        self.pending.insert(fragment.sequence_id, fragment);
        let mut ready = Vec::new();
        while let Some(fragment) = self.pending.remove(&self.next_sequence) {
            self.next_sequence += 1;
            self.gap_started = None;
            ready.push(fragment);
        }
        if !self.pending.is_empty() && self.gap_started.is_none() {
            self.gap_started = Some(now);
        }
        ready
    }
    pub fn gap_expired(&self, now: Instant) -> bool {
        self.gap_started
            .map(|at| now.duration_since(at) >= self.max_gap_wait)
            .unwrap_or(false)
    }
    pub fn take_gap_marker(&mut self) -> Option<u64> {
        let first = *self.pending.keys().next()?;
        self.next_sequence = first;
        self.gap_started = None;
        Some(first)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Readiness {
    Collecting,
    WaitingForContext,
    ReadyForGeneration,
    Generating,
    Streaming,
    Completed,
    Superseded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadinessDecision {
    pub state: Readiness,
    pub missing_dependencies: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DependencyFingerprint {
    pub graph_version: u64,
    pub revision: u64,
    pub generation_epoch: u64,
}

impl DependencyFingerprint {
    pub fn is_current(self, graph_version: u64, revision: u64, generation_epoch: u64) -> bool {
        self.graph_version == graph_version
            && self.revision == revision
            && self.generation_epoch == generation_epoch
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryCheckpoint {
    pub session_id: String,
    pub summary: String,
    pub summary_version: u64,
    pub covered_turn_id: u64,
    pub owner_epoch: u64,
}

impl SummaryCheckpoint {
    pub fn can_apply(&self, graph_version: u64, owner_epoch: u64) -> bool {
        self.owner_epoch == owner_epoch && self.summary_version <= graph_version
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConversationNodeReference {
    pub node_id: String,
    pub event_id: String,
    pub main_title: String,
    pub kind: ConversationNodeKind,
    pub turn_id: u64,
    pub revision: u64,
    pub graph_version: u64,
    pub interrupted: bool,
    pub replay_prompt_pending: bool,
}

impl ConversationNodeReference {
    pub fn from_node(node: &ConversationNode) -> Self {
        let normalized = node
            .content
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let preview = normalized.chars().take(80).collect::<String>();
        let main_title = match node.kind {
            ConversationNodeKind::UserText => format!("User: {preview}"),
            ConversationNodeKind::OptimizedIntent => format!("Intent: {preview}"),
            ConversationNodeKind::ToolRequest => format!("Tool request: {preview}"),
            ConversationNodeKind::ToolContext => format!("Tool context: {preview}"),
            ConversationNodeKind::AssistantResponse if node.interrupted => {
                format!("Assistant (interrupted): {preview}")
            }
            ConversationNodeKind::AssistantResponse => format!("Assistant: {preview}"),
            ConversationNodeKind::AssistantInterrupted => "Assistant interrupted".to_string(),
            ConversationNodeKind::GraphEnrichment => format!("Graph context: {preview}"),
        };
        Self {
            node_id: node.id.clone(),
            event_id: node.event_id.clone(),
            main_title,
            kind: node.kind.clone(),
            turn_id: node.turn_id,
            revision: node.revision,
            graph_version: node.graph_version,
            interrupted: node.interrupted,
            replay_prompt_pending: node
                .interruption_state
                .as_ref()
                .is_some_and(|state| state.replay_prompt_pending),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConversationNodeKind {
    UserText,
    OptimizedIntent,
    ToolRequest,
    ToolContext,
    AssistantResponse,
    AssistantInterrupted,
    /// Compact, asynchronously produced relation/intent annotation. It is
    /// never caller-facing and is revision-checked by the SessionActor.
    GraphEnrichment,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationNode {
    pub id: String,
    pub event_id: String,
    pub created_at: String,
    pub kind: ConversationNodeKind,
    pub turn_id: u64,
    pub revision: u64,
    pub content: String,
    pub parent_ids: Vec<String>,
    pub graph_version: u64,
    pub ready: bool,
    /// Revision consumed by the reconstruction LLM. This is intentionally
    /// separate from whether the caller has heard the response.
    #[serde(default)]
    pub llm_seen_revision: u64,
    /// Revision spoken through STS to the caller.
    #[serde(default)]
    pub user_spoken_revision: u64,
    /// True when playback stopped before the caller heard the complete response.
    #[serde(default)]
    pub interrupted: bool,
    #[serde(default)]
    pub interruption_state: Option<InterruptionState>,
    /// Stable source identity used by graph restructuring/upsert operations.
    #[serde(default)]
    pub source_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InterruptionState {
    pub generation_id: u64,
    pub timestamp_ms: u64,
    pub replay_prompt_pending: bool,
    #[serde(default)]
    pub response_node_id: Option<String>,
    #[serde(default)]
    pub query_node_id: Option<String>,
    #[serde(default)]
    pub response_ready: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConversationGraph {
    pub session_id: String,
    pub graph_version: u64,
    pub next_turn_id: u64,
    pub nodes: BTreeMap<String, ConversationNode>,
    pub latest_user_node: Option<String>,
    pub latest_context_node: Option<String>,
}

/// Compact, actor-owned working memory derived from the conversation graph.
/// Unlike the graph this is optimized for deterministic follow-up resolution,
/// not auditing every event and relationship.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionMemory {
    pub graph_version: u64,
    pub active_topic: Option<String>,
    pub active_entity_ids: Vec<String>,
    pub selected_entity_id: Option<String>,
    pub ordinal_map: BTreeMap<String, String>,
    pub latest_verified_result_node_id: Option<String>,
    pub recent_turns: Vec<SessionMemoryTurn>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMemoryTurn {
    pub node_id: String,
    pub kind: ConversationNodeKind,
    pub turn_id: u64,
    pub content: String,
    pub verified: bool,
}

impl SessionMemory {
    /// Rebuild deterministic working memory from graph events. This accepts
    /// malformed or non-JSON tool payloads: they simply contribute no entity
    /// IDs while still remaining available through the graph audit trail.
    pub fn from_conversation_graph(graph: &ConversationGraph) -> Self {
        let mut nodes = graph.nodes.values().collect::<Vec<_>>();
        nodes.sort_by(|left, right| {
            left.turn_id
                .cmp(&right.turn_id)
                .then_with(|| left.graph_version.cmp(&right.graph_version))
                .then_with(|| left.id.cmp(&right.id))
        });

        let active_topic = nodes
            .iter()
            .rev()
            .find(|node| node.kind == ConversationNodeKind::UserText)
            .map(|node| node.content.clone());
        let latest_tool = nodes
            .iter()
            .rev()
            .find(|node| node.kind == ConversationNodeKind::ToolContext);
        let mut active_entity_ids = latest_tool
            .and_then(|node| serde_json::from_str::<serde_json::Value>(&node.content).ok())
            .map(|value| extract_entity_ids(&value))
            .unwrap_or_default();
        let mut seen_entities = BTreeSet::new();
        active_entity_ids.retain(|id| seen_entities.insert(id.clone()));

        let ordinal_map = active_entity_ids
            .iter()
            .enumerate()
            .filter_map(|(index, id)| ordinal_name(index).map(|ordinal| (ordinal, id.clone())))
            .collect::<BTreeMap<_, _>>();
        let selected_entity_id = active_entity_ids.first().cloned();
        let recent_nodes = nodes
            .iter()
            .filter(|node| {
                matches!(
                    node.kind,
                    ConversationNodeKind::UserText
                        | ConversationNodeKind::AssistantResponse
                        | ConversationNodeKind::ToolContext
                )
            })
            .collect::<Vec<_>>();
        let recent_start = recent_nodes.len().saturating_sub(16);
        let recent_turns = recent_nodes[recent_start..]
            .iter()
            .map(|node| SessionMemoryTurn {
                node_id: node.id.clone(),
                kind: node.kind.clone(),
                turn_id: node.turn_id,
                content: node.content.clone(),
                verified: node.kind == ConversationNodeKind::ToolContext,
            })
            .collect();

        Self {
            graph_version: graph.graph_version,
            active_topic,
            active_entity_ids,
            selected_entity_id,
            ordinal_map,
            latest_verified_result_node_id: latest_tool.map(|node| node.id.clone()),
            recent_turns,
        }
    }
}

fn ordinal_name(index: usize) -> Option<String> {
    const ORDINALS: [&str; 20] = [
        "first",
        "second",
        "third",
        "fourth",
        "fifth",
        "sixth",
        "seventh",
        "eighth",
        "ninth",
        "tenth",
        "eleventh",
        "twelfth",
        "thirteenth",
        "fourteenth",
        "fifteenth",
        "sixteenth",
        "seventeenth",
        "eighteenth",
        "nineteenth",
        "twentieth",
    ];
    ORDINALS.get(index).map(|value| (*value).to_string())
}

fn extract_entity_ids(value: &serde_json::Value) -> Vec<String> {
    fn visit(value: &serde_json::Value, ids: &mut Vec<String>) {
        match value {
            serde_json::Value::Array(values) => values.iter().for_each(|value| visit(value, ids)),
            serde_json::Value::Object(values) => {
                for key in ["listing_id", "property_id", "entity_id", "id"] {
                    if let Some(value) = values.get(key).and_then(|value| value.as_str()) {
                        if !value.trim().is_empty() {
                            ids.push(value.trim().to_string());
                        }
                    }
                }
                values.values().for_each(|value| visit(value, ids));
            }
            _ => {}
        }
    }
    let mut ids = Vec::new();
    visit(value, &mut ids);
    ids
}

impl ConversationGraph {
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            ..Self::default()
        }
    }

    pub fn reset(&mut self) {
        self.graph_version = self.graph_version.saturating_add(1);
        self.next_turn_id = 0;
        self.nodes.clear();
        self.latest_user_node = None;
        self.latest_context_node = None;
    }

    /// Build a bounded, LLM-oriented memory view from the actor-owned graph.
    ///
    /// The graph remains the source of truth.  This view deliberately makes
    /// the current subject, verified tool result, and recent conversational
    /// turns explicit so callers do not need to infer them from raw topology
    /// or a DashMap title alone.
    pub fn memory_snapshot(&self, max_turns: usize) -> serde_json::Value {
        let mut ordered = self.nodes.values().collect::<Vec<_>>();
        ordered.sort_by(|left, right| {
            left.turn_id
                .cmp(&right.turn_id)
                .then_with(|| left.graph_version.cmp(&right.graph_version))
                .then_with(|| left.id.cmp(&right.id))
        });

        let recent_nodes = ordered
            .iter()
            .filter(|node| {
                matches!(
                    node.kind,
                    ConversationNodeKind::UserText
                        | ConversationNodeKind::AssistantResponse
                        | ConversationNodeKind::ToolContext
                )
            })
            .collect::<Vec<_>>();
        let recent_start = recent_nodes.len().saturating_sub(max_turns);
        let recent_turns = recent_nodes[recent_start..]
            .iter()
            .map(|node| {
                serde_json::json!({
                    "node_id": node.id,
                    "kind": node.kind,
                    "turn_id": node.turn_id,
                    "content": node.content,
                    "verified": node.kind == ConversationNodeKind::ToolContext,
                })
            })
            .collect::<Vec<_>>();

        let active_subject = ordered
            .iter()
            .rev()
            .find(|node| {
                matches!(
                    node.kind,
                    ConversationNodeKind::ToolContext | ConversationNodeKind::AssistantResponse
                )
            })
            .map(|node| {
                serde_json::json!({
                    "node_id": node.id,
                    "kind": node.kind,
                    "turn_id": node.turn_id,
                    "content": node.content,
                    "verified": node.kind == ConversationNodeKind::ToolContext,
                })
            })
            .unwrap_or(serde_json::Value::Null);

        let latest_verified_result = ordered
            .iter()
            .rev()
            .find(|node| node.kind == ConversationNodeKind::ToolContext)
            .map(|node| {
                serde_json::json!({
                    "node_id": node.id,
                    "turn_id": node.turn_id,
                    "content": node.content,
                })
            })
            .unwrap_or(serde_json::Value::Null);

        serde_json::json!({
            "active_subject": active_subject,
            "latest_verified_result": latest_verified_result,
            "recent_turns": recent_turns,
            "instruction": "Use active_subject and latest_verified_result for pronouns, ordinals, and prior-discussion questions. ToolContext content is verified data; AssistantResponse content records what was said to the caller.",
        })
    }

    pub fn append_user_text(&mut self, text: &str) -> Option<String> {
        let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
        // Transcript sanitization already rejects noise. Keep valid short
        // contextual turns such as "no", "yes", and "hi" so they reach the
        // agent instead of being silently dropped by graph intake.
        if normalized.is_empty() {
            return None;
        }
        // A follow-up may refer to the last tool result whose answer was cut
        // off. Re-open only that unheard branch for graph reconciliation; this
        // avoids replaying already-heard context or continually regenerating a
        // response while it is being spoken.
        if let Some(context_id) = self.latest_context_node.clone() {
            if let Some(context) = self.nodes.get_mut(&context_id) {
                if context.user_spoken_revision < context.revision {
                    context.llm_seen_revision = 0;
                }
            }
        }
        self.graph_version = self.graph_version.saturating_add(1);
        self.next_turn_id = self.next_turn_id.saturating_add(1);
        let id = format!("user-{}-{}", self.next_turn_id, self.graph_version);
        let node = ConversationNode {
            id: id.clone(),
            event_id: uuid::Uuid::new_v4().to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            kind: ConversationNodeKind::UserText,
            turn_id: self.next_turn_id,
            revision: 1,
            content: normalized,
            // A new user query is linked to the latest resolved context when
            // available, otherwise to the preceding user turn. This preserves
            // a pending-tool dependency chain so the fast planner can decide
            // whether a follow-up should wait for that work or supersede it.
            parent_ids: self
                .latest_context_node
                .clone()
                .or_else(|| self.latest_user_node.clone())
                .into_iter()
                .collect(),
            graph_version: self.graph_version,
            ready: true,
            llm_seen_revision: 1,
            user_spoken_revision: 1,
            interrupted: false,
            interruption_state: None,
            source_key: None,
        };
        self.nodes.insert(id.clone(), node);
        self.latest_user_node = Some(id.clone());
        Some(id)
    }

    pub fn append_optimized_intent(
        &mut self,
        content: impl Into<String>,
        parent_id: &str,
    ) -> Option<String> {
        let content = content.into();
        if content.trim().is_empty() || !self.nodes.contains_key(parent_id) {
            return None;
        }
        self.graph_version = self.graph_version.saturating_add(1);
        let id = format!("intent-{}", self.graph_version);
        self.nodes.insert(
            id.clone(),
            ConversationNode {
                id: id.clone(),
                event_id: uuid::Uuid::new_v4().to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
                kind: ConversationNodeKind::OptimizedIntent,
                turn_id: self.next_turn_id,
                revision: 1,
                content,
                parent_ids: vec![parent_id.to_string()],
                graph_version: self.graph_version,
                ready: true,
                llm_seen_revision: 1,
                user_spoken_revision: 1,
                interrupted: false,
                interruption_state: None,
                source_key: None,
            },
        );
        Some(id)
    }

    /// Record every caller-facing assistant prompt, including clarification
    /// and suggestion turns that have no tool context. Later confirmations
    /// can therefore resolve "go with that" from graph structure.
    pub fn append_assistant_response(
        &mut self,
        content: impl Into<String>,
        source_key: Option<String>,
    ) -> Option<String> {
        let content = content.into();
        if content.trim().is_empty() {
            return None;
        }
        if let Some(key) = source_key.as_deref() {
            if self.nodes.values().any(|node| {
                node.kind == ConversationNodeKind::AssistantResponse
                    && node.source_key.as_deref() == Some(key)
            }) {
                return None;
            }
        }
        self.graph_version = self.graph_version.saturating_add(1);
        let id = format!("response-{}", self.graph_version);
        let parent_ids = self
            .latest_context_node
            .clone()
            .or_else(|| self.latest_user_node.clone())
            .into_iter()
            .collect();
        self.nodes.insert(
            id.clone(),
            ConversationNode {
                id: id.clone(),
                event_id: uuid::Uuid::new_v4().to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
                kind: ConversationNodeKind::AssistantResponse,
                turn_id: self.next_turn_id,
                revision: 1,
                content,
                parent_ids,
                graph_version: self.graph_version,
                ready: true,
                llm_seen_revision: 1,
                user_spoken_revision: 0,
                interrupted: false,
                interruption_state: None,
                source_key,
            },
        );
        self.latest_context_node = Some(id.clone());
        Some(id)
    }

    /// Records that playback for an assistant generation was cut off by caller
    /// speech.  This is a graph event, rather than an assistant response, so a
    /// later context-aware answer knows that the caller did not hear the whole
    /// previous turn.  The generation key also makes duplicate barge-in signals
    /// (from VAD and the transport) idempotent.
    pub fn append_assistant_interrupted(
        &mut self,
        generation_id: u64,
        timestamp_ms: u64,
    ) -> Option<String> {
        let query_node_id = self.latest_user_node.clone();
        let response_node_id = self
            .nodes
            .iter()
            .filter(|(_, node)| {
                node.kind == ConversationNodeKind::AssistantResponse
                    && !node.interrupted
                    && node.user_spoken_revision < node.revision
            })
            .max_by_key(|(_, node)| node.graph_version)
            .map(|(id, _)| id.clone());
        self.append_assistant_interrupted_for_nodes(
            generation_id,
            timestamp_ms,
            response_node_id.as_deref(),
            query_node_id.as_deref(),
        )
    }

    pub fn append_assistant_interrupted_for_nodes(
        &mut self,
        generation_id: u64,
        timestamp_ms: u64,
        response_node_id: Option<&str>,
        query_node_id: Option<&str>,
    ) -> Option<String> {
        let source_key = format!("assistant-interrupted:{generation_id}");
        let existing_interruption = self.nodes.iter().find_map(|(id, node)| {
            (node.source_key.as_deref() == Some(source_key.as_str())
                && node.kind == ConversationNodeKind::AssistantInterrupted)
                .then_some(id.clone())
        });
        let existing_state = existing_interruption
            .as_ref()
            .and_then(|id| self.nodes.get(id))
            .and_then(|node| node.interruption_state.as_ref())
            .cloned();
        let response_node_id = response_node_id
            .filter(|id| {
                self.nodes
                    .get(*id)
                    .is_some_and(|node| node.kind == ConversationNodeKind::AssistantResponse)
            })
            .map(str::to_string)
            .or_else(|| {
                existing_state
                    .as_ref()
                    .and_then(|state| state.response_node_id.clone())
            });
        let query_node_id = query_node_id
            .filter(|id| {
                self.nodes
                    .get(*id)
                    .is_some_and(|node| node.kind == ConversationNodeKind::UserText)
            })
            .map(str::to_string)
            .or_else(|| {
                existing_state
                    .as_ref()
                    .and_then(|state| state.query_node_id.clone())
            });
        let state = InterruptionState {
            generation_id,
            timestamp_ms,
            replay_prompt_pending: true,
            response_ready: response_node_id.is_some(),
            response_node_id: response_node_id.clone(),
            query_node_id: query_node_id.clone(),
        };

        if let Some(interruption_id) = existing_interruption {
            if existing_state.as_ref().is_some_and(|existing| {
                existing.response_node_id == state.response_node_id
                    && existing.query_node_id == state.query_node_id
                    && existing.response_ready == state.response_ready
            }) {
                return None;
            }
            self.graph_version = self.graph_version.saturating_add(1);
            if let Some(response_id) = &response_node_id {
                if let Some(response) = self.nodes.get_mut(response_id) {
                    response.interrupted = true;
                    response.interruption_state = Some(state.clone());
                    response.revision = response.revision.saturating_add(1);
                    response.graph_version = self.graph_version;
                }
            }
            if let Some(node) = self.nodes.get_mut(&interruption_id) {
                node.interruption_state = Some(state);
                node.parent_ids = response_node_id
                    .iter()
                    .chain(query_node_id.iter())
                    .cloned()
                    .collect();
                node.revision = node.revision.saturating_add(1);
                node.graph_version = self.graph_version;
            }
            self.latest_context_node = Some(interruption_id.clone());
            return Some(interruption_id);
        }

        self.graph_version = self.graph_version.saturating_add(1);
        if let Some(response_id) = &response_node_id {
            if let Some(response) = self.nodes.get_mut(response_id) {
                response.interrupted = true;
                response.interruption_state = Some(state.clone());
                response.revision = response.revision.saturating_add(1);
                response.graph_version = self.graph_version;
            }
        }
        let id = format!("interrupted-{}", self.graph_version);
        let parent_ids = response_node_id
            .iter()
            .chain(query_node_id.iter())
            .cloned()
            .into_iter()
            .collect();
        self.nodes.insert(
            id.clone(),
            ConversationNode {
                id: id.clone(),
                event_id: uuid::Uuid::new_v4().to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
                kind: ConversationNodeKind::AssistantInterrupted,
                turn_id: self.next_turn_id,
                revision: 1,
                content: format!(
                    "Caller interrupted assistant playback at {timestamp_ms} ms (generation {generation_id})."
                ),
                parent_ids,
                graph_version: self.graph_version,
                ready: true,
                llm_seen_revision: 1,
                user_spoken_revision: 0,
                interrupted: false,
                interruption_state: Some(state),
                source_key: Some(source_key),
            },
        );
        self.latest_context_node = Some(id.clone());
        Some(id)
    }

    pub fn mark_interruption_resumed(
        &mut self,
        response_node_id: &str,
        generation_id: u64,
    ) -> bool {
        let Some(response) = self.nodes.get(response_node_id) else {
            return false;
        };
        let matches_generation = response
            .interruption_state
            .as_ref()
            .is_some_and(|state| state.generation_id == generation_id);
        if !matches_generation {
            return false;
        }

        self.graph_version = self.graph_version.saturating_add(1);
        if let Some(response) = self.nodes.get_mut(response_node_id) {
            response.interrupted = false;
            if let Some(state) = &mut response.interruption_state {
                state.replay_prompt_pending = false;
            }
            response.revision = response.revision.saturating_add(1);
            response.graph_version = self.graph_version;
        }
        let interruption_key = format!("assistant-interrupted:{generation_id}");
        for node in self.nodes.values_mut().filter(|node| {
            node.kind == ConversationNodeKind::AssistantInterrupted
                && node.source_key.as_deref() == Some(interruption_key.as_str())
        }) {
            if let Some(state) = &mut node.interruption_state {
                state.replay_prompt_pending = false;
            }
            node.revision = node.revision.saturating_add(1);
            node.graph_version = self.graph_version;
        }
        true
    }

    /// Store an asynchronous relation annotation on the immutable user-query
    /// branch.  `base_graph_version` is an optimistic-concurrency fence: a
    /// worker cannot attach output to a query that no longer exists, while
    /// harmless unrelated graph updates do not discard useful enrichment.
    pub fn upsert_graph_enrichment_for_query(
        &mut self,
        query_id: &str,
        base_graph_version: u64,
        content: impl Into<String>,
    ) -> Option<String> {
        let content = content.into();
        if content.trim().is_empty() || base_graph_version > self.graph_version {
            return None;
        }
        let (parent_id, turn_id) = self.nodes.iter().find_map(|(id, node)| {
            (node.kind == ConversationNodeKind::UserText && node.event_id == query_id)
                .then_some((id.clone(), node.turn_id))
        })?;
        let source_key = format!("graph-enrichment:{query_id}");
        if let Some((id, node)) = self.nodes.iter_mut().find(|(_, node)| {
            node.kind == ConversationNodeKind::GraphEnrichment
                && node.source_key.as_deref() == Some(source_key.as_str())
        }) {
            if node.content != content {
                self.graph_version = self.graph_version.saturating_add(1);
                node.content = content;
                node.revision = node.revision.saturating_add(1);
                node.graph_version = self.graph_version;
            }
            return Some(id.clone());
        }
        self.graph_version = self.graph_version.saturating_add(1);
        let id = format!("enrichment-{}", self.graph_version);
        self.nodes.insert(
            id.clone(),
            ConversationNode {
                id: id.clone(),
                event_id: uuid::Uuid::new_v4().to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
                kind: ConversationNodeKind::GraphEnrichment,
                turn_id,
                revision: 1,
                content,
                parent_ids: vec![parent_id],
                graph_version: self.graph_version,
                ready: true,
                llm_seen_revision: 1,
                user_spoken_revision: 1,
                interrupted: false,
                interruption_state: None,
                source_key: Some(source_key),
            },
        );
        Some(id)
    }

    pub fn append_tool_context(
        &mut self,
        content: impl Into<String>,
        parent_ids: Vec<String>,
    ) -> Option<String> {
        let content = content.into();
        if content.trim().is_empty() {
            return None;
        }
        self.upsert_tool_context(content, parent_ids, None)
    }

    /// Merge tool/RAG data into the graph. A stable source key updates the
    /// existing node instead of creating duplicates when a tool retries or
    /// returns a refined result.
    pub fn upsert_tool_context(
        &mut self,
        content: impl Into<String>,
        parent_ids: Vec<String>,
        source_key: Option<String>,
    ) -> Option<String> {
        self.upsert_tool_context_for_turn(content, parent_ids, source_key, self.next_turn_id)
    }

    /// Upsert tool data into the turn that issued the request. A tool can finish
    /// after later user turns arrive; its result must not be relabeled as data
    /// for the newest turn merely because it completed later.
    pub fn upsert_tool_context_for_turn(
        &mut self,
        content: impl Into<String>,
        parent_ids: Vec<String>,
        source_key: Option<String>,
        turn_id: u64,
    ) -> Option<String> {
        let content = content.into();
        if content.trim().is_empty() {
            return None;
        }
        if let Some(key) = source_key.as_deref() {
            if let Some((id, node)) = self.nodes.iter_mut().find(|(_, node)| {
                node.kind == ConversationNodeKind::ToolContext
                    && node.source_key.as_deref() == Some(key)
            }) {
                if node.content != content || node.parent_ids != parent_ids {
                    self.graph_version = self.graph_version.saturating_add(1);
                    node.content = content;
                    node.parent_ids = parent_ids;
                    node.revision = node.revision.saturating_add(1);
                    node.graph_version = self.graph_version;
                    node.ready = true;
                }
                self.latest_context_node = Some(id.clone());
                return Some(id.clone());
            }
        }
        self.graph_version = self.graph_version.saturating_add(1);
        let id = format!("context-{}", self.graph_version);
        self.nodes.insert(
            id.clone(),
            ConversationNode {
                id: id.clone(),
                event_id: uuid::Uuid::new_v4().to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
                kind: ConversationNodeKind::ToolContext,
                turn_id,
                revision: 1,
                content,
                parent_ids,
                graph_version: self.graph_version,
                ready: true,
                llm_seen_revision: 0,
                user_spoken_revision: 0,
                interrupted: false,
                interruption_state: None,
                source_key,
            },
        );
        self.latest_context_node = Some(id.clone());
        Some(id)
    }

    pub fn response_context(&self) -> Option<(String, String)> {
        let question = self
            .latest_user_node
            .as_ref()
            .and_then(|id| self.nodes.get(id))
            .map(|node| node.content.clone())?;
        let context = self
            .nodes
            .values()
            .filter(|node| {
                node.turn_id == self.next_turn_id
                    && node.llm_seen_revision < node.revision
                    && matches!(node.kind, ConversationNodeKind::ToolContext)
            })
            .map(|node| node.content.clone())
            .collect::<Vec<_>>()
            .join("\n");
        (!context.trim().is_empty()).then_some((question, context))
    }

    pub fn pending_llm_node_ids(&self) -> Vec<String> {
        self.nodes
            .values()
            .filter(|node| {
                matches!(node.kind, ConversationNodeKind::ToolContext)
                    && node.llm_seen_revision < node.revision
                    // Current tool data is always eligible. Older context that
                    // the caller never heard is also eligible, so a follow-up
                    // can be answered from the outstanding branch instead of
                    // blindly repeating a new RAG payload.
                    && (node.turn_id == self.next_turn_id
                        || node.user_spoken_revision < node.revision)
            })
            .map(|node| node.id.clone())
            .collect()
    }

    pub fn mark_llm_seen(&mut self, node_ids: &[String]) {
        for id in node_ids {
            if let Some(node) = self.nodes.get_mut(id) {
                node.llm_seen_revision = node.revision;
            }
        }
    }

    /// Mark the latest delivered assistant response and the tool-context nodes
    /// it depends on as actually heard. This is deliberately separate from
    /// `mark_llm_seen`: model consumption does not mean the caller heard the
    /// answer. If playback is interrupted, this method is not called and the
    /// related context remains eligible for a later query-aware reconstruction.
    pub fn mark_latest_response_spoken(&mut self) -> bool {
        let Some(response_id) = self
            .nodes
            .iter()
            .filter(|(_, node)| {
                matches!(node.kind, ConversationNodeKind::AssistantResponse)
                    && !node.interrupted
                    && node.user_spoken_revision < node.revision
            })
            .max_by_key(|(_, node)| node.graph_version)
            .map(|(id, _)| id.clone())
        else {
            return false;
        };

        let mut pending = vec![response_id];
        let mut visited = std::collections::BTreeSet::new();
        while let Some(node_id) = pending.pop() {
            if !visited.insert(node_id.clone()) {
                continue;
            }
            let Some(node) = self.nodes.get(&node_id) else {
                continue;
            };
            pending.extend(node.parent_ids.iter().cloned());
        }
        let mut changed = false;
        for node_id in visited {
            if let Some(node) = self.nodes.get_mut(&node_id) {
                if let Some(interruption) = &mut node.interruption_state {
                    if interruption.replay_prompt_pending {
                        interruption.replay_prompt_pending = false;
                        changed = true;
                    }
                }
                if matches!(
                    node.kind,
                    ConversationNodeKind::AssistantResponse | ConversationNodeKind::ToolContext
                ) && node.user_spoken_revision < node.revision
                {
                    node.user_spoken_revision = node.revision;
                    changed = true;
                }
            }
        }
        changed
    }

    pub fn snapshot_json(&self) -> String {
        serde_json::json!({
            "graph_version": self.graph_version,
            "latest_user_node": self.latest_user_node,
            "latest_context_node": self.latest_context_node,
            // Keep the full bounded session graph so parent/child relations
            // remain reconstructable across turns and tool updates.
            "nodes": self.nodes.values().collect::<Vec<_>>(),
        })
        .to_string()
    }

    /// Snapshot only the graph component that can influence the latest answer:
    /// the current user turn, its unresolved tool context, and every connected
    /// ancestor/descendant. This preserves parent relationships while keeping
    /// unrelated prior searches out of the reconstruction prompt.
    pub fn relevant_snapshot_json(&self) -> String {
        let Some(query_id) = self
            .latest_user_node
            .as_ref()
            .and_then(|id| self.nodes.get(id))
            .map(|node| node.event_id.as_str())
        else {
            return String::new();
        };
        self.relevant_snapshot_json_for_query(query_id)
            .unwrap_or_default()
    }

    /// Build a graph payload only when a worker explicitly asks for it. The
    /// payload is the connected component for one user query, never the full
    /// session graph or an unrelated conversation dump.
    pub fn relevant_snapshot_json_for_query(&self, query_id: &str) -> Option<String> {
        let user_id = self
            .nodes
            .iter()
            .find(|(_, node)| {
                matches!(node.kind, ConversationNodeKind::UserText) && node.event_id == query_id
            })
            .map(|(id, _)| id.clone())?;
        let mut related = std::collections::BTreeSet::new();
        related.insert(user_id.clone());
        // Follow edges in both directions until the connected component is
        // complete. Parent IDs are the graph's dependency relation.
        loop {
            let before = related.len();
            for (id, node) in &self.nodes {
                if related.contains(id)
                    || node
                        .parent_ids
                        .iter()
                        .any(|parent| related.contains(parent))
                {
                    related.insert(id.clone());
                    related.extend(node.parent_ids.iter().cloned());
                }
            }
            if related.len() == before {
                break;
            }
        }
        let nodes = related
            .iter()
            .filter_map(|id| self.nodes.get(id))
            .collect::<Vec<_>>();
        // Parent IDs preserve topology; add explicit semantic edge labels so
        // fast/main models need not infer relationship meaning from ordering.
        let relations = nodes
            .iter()
            .flat_map(|node| {
                let relation = match node.kind {
                    ConversationNodeKind::UserText => "follows_or_refines",
                    ConversationNodeKind::OptimizedIntent => "resolves_intent_for",
                    ConversationNodeKind::ToolRequest => "requests_tool_for",
                    ConversationNodeKind::ToolContext => "verified_result_for",
                    ConversationNodeKind::AssistantResponse => "answers_or_suggests_for",
                    ConversationNodeKind::AssistantInterrupted => "interrupts_playback_of",
                    ConversationNodeKind::GraphEnrichment => "enriches_context_for",
                };
                node.parent_ids.iter().map(move |parent_id| {
                    serde_json::json!({
                        "from": node.id,
                        "to": parent_id,
                        "relation": relation,
                        "source_event_id": node.event_id,
                    })
                })
            })
            .collect::<Vec<_>>();
        Some(
            serde_json::json!({
                "graph_version": self.graph_version,
                "query_id": query_id,
                "latest_user_node": user_id,
                "latest_context_node": self.latest_context_node,
                "nodes": nodes,
                "relations": relations,
            })
            .to_string(),
        )
    }
}

/// Cheap, deterministic gate used before any model/tool work. It keeps
/// incomplete utterances event-driven without consuming worker capacity.
pub fn evaluate_readiness(
    stable_asr: bool,
    required_fields: &[(&str, bool)],
    pending_tools: &[String],
    context_ready: bool,
) -> ReadinessDecision {
    if !stable_asr {
        return ReadinessDecision {
            state: Readiness::Collecting,
            missing_dependencies: vec!["stable_asr".into()],
        };
    }
    let mut missing = required_fields
        .iter()
        .filter(|(_, present)| !*present)
        .map(|(name, _)| (*name).to_string())
        .collect::<Vec<_>>();
    missing.extend(pending_tools.iter().cloned());
    if !context_ready {
        missing.push("response_context".into());
    }
    if missing.is_empty() {
        ReadinessDecision {
            state: Readiness::ReadyForGeneration,
            missing_dependencies: missing,
        }
    } else {
        ReadinessDecision {
            state: Readiness::WaitingForContext,
            missing_dependencies: missing,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReadyWork {
    pub session_id: String,
    pub node_id: String,
    pub turn_id: u64,
    pub generation_id: u64,
    pub priority: u8,
    pub enqueue_sequence: u64,
    pub deadline: Option<Instant>,
    pub graph_version: u64,
}

#[derive(Debug, Default)]
pub struct FairReadyScheduler {
    queues: HashMap<String, Vec<ReadyWork>>,
    cursor: usize,
}

impl FairReadyScheduler {
    pub fn push(&mut self, work: ReadyWork) {
        self.queues
            .entry(work.session_id.clone())
            .or_default()
            .push(work);
    }
    pub fn pop(&mut self) -> Option<ReadyWork> {
        let mut ids: Vec<_> = self.queues.keys().cloned().collect();
        ids.sort();
        if ids.is_empty() {
            return None;
        }
        for offset in 0..ids.len() {
            let idx = (self.cursor + offset) % ids.len();
            let id = &ids[idx];
            if let Some(queue) = self.queues.get_mut(id) {
                if !queue.is_empty() {
                    self.cursor = (idx + 1) % ids.len();
                    let best = queue
                        .iter()
                        .enumerate()
                        .min_by_key(|(_, w)| (w.priority, w.enqueue_sequence))
                        .map(|(i, _)| i)
                        .unwrap();
                    let item = queue.remove(best);
                    if queue.is_empty() {
                        self.queues.remove(id);
                    }
                    return Some(item);
                }
            }
        }
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeechSegment {
    pub turn_id: u64,
    pub generation_id: u64,
    pub output_sequence: u64,
    pub text: String,
    pub stable: bool,
}

#[derive(Debug, Default)]
pub struct OutputSequencer {
    next: HashMap<(u64, u64), u64>,
    buffered: BTreeMap<(u64, u64, u64), SpeechSegment>,
    cancelled: BTreeSet<(u64, u64)>,
}

#[cfg(test)]
#[path = "../../../../tests/workflow_workflow_lib_graph_reference_tests_1.rs"]
mod graph_reference_tests;

impl OutputSequencer {
    pub fn cancel_generation(&mut self, turn_id: u64, generation_id: u64) {
        self.cancelled.insert((turn_id, generation_id));
        self.buffered
            .retain(|(t, g, _), _| (*t, *g) != (turn_id, generation_id));
    }
    pub fn push(&mut self, segment: SpeechSegment) -> Vec<SpeechSegment> {
        let key = (segment.turn_id, segment.generation_id);
        if self.cancelled.contains(&key) || !segment.stable {
            return Vec::new();
        }
        self.buffered.insert(
            (
                segment.turn_id,
                segment.generation_id,
                segment.output_sequence,
            ),
            segment,
        );
        let expected = self.next.entry(key).or_insert(0);
        let mut out = Vec::new();
        while let Some(segment) = self.buffered.remove(&(key.0, key.1, *expected)) {
            out.push(segment);
            *expected += 1;
        }
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolTerminalState {
    Running,
    Completed,
    TimedOut,
    Cancelled,
    UnknownOutcome,
}

#[derive(Debug, Clone)]
pub struct ToolRegistration {
    pub session_id: String,
    pub node_id: String,
    pub call_id: String,
    pub generation: u64,
    pub owner_epoch: u64,
    pub deadline: Instant,
    pub state: ToolTerminalState,
}

#[derive(Debug, Default)]
pub struct ToolRegistry {
    entries: HashMap<String, ToolRegistration>,
}

impl ToolRegistry {
    pub fn register(&mut self, entry: ToolRegistration) -> bool {
        if self.entries.contains_key(&entry.call_id) {
            return false;
        }
        self.entries.insert(entry.call_id.clone(), entry);
        true
    }
    pub fn finish(
        &mut self,
        call_id: &str,
        session_id: &str,
        node_id: &str,
        generation: u64,
        owner_epoch: u64,
        state: ToolTerminalState,
    ) -> bool {
        let Some(entry) = self.entries.get_mut(call_id) else {
            return false;
        };
        if entry.session_id != session_id
            || entry.node_id != node_id
            || entry.generation != generation
            || entry.owner_epoch != owner_epoch
            || entry.state != ToolTerminalState::Running
        {
            return false;
        }
        entry.state = state;
        true
    }
    pub fn remove(&mut self, call_id: &str) -> Option<ToolRegistration> {
        self.entries.remove(call_id)
    }
    pub fn get(&self, call_id: &str) -> Option<&ToolRegistration> {
        self.entries.get(call_id)
    }
}
