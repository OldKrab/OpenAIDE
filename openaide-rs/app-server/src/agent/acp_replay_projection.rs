use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::agent::acp_schema::{ContentBlock, SessionUpdate};

use crate::agent::acp_codex_subagent::{
    project_codex_collaboration, CodexSubagentProjection, CodexSubagentState,
};
use crate::agent::acp_content_projection::project_content_block;
use crate::agent::acp_live_prompt_projection::project_plan_entry;
use crate::agent::acp_message_identity::{stable_agent_message_id, stable_message_id};
use crate::agent::acp_tool_call_projection::{
    merge_tool_call_update, remember_tool_call, ToolCallState,
};
use crate::agent::normalizer::normalize_event;
use crate::agent::tool_details::tool_call_event;
use crate::agent::AgentReplayPlanState;
use crate::protocol::model::{
    AgentMessagePart, AgentMessageRole, AgentPlan, AgentPlanStatus, NormalizedMessage,
};
use crate::time::now_string;

pub(super) struct ReplayProjection {
    agent_id: Option<String>,
    session_id: String,
}

#[derive(Default)]
pub(super) struct ReplayProjectionResult {
    pub(super) messages: Vec<NormalizedMessage>,
    pub(super) plan: AgentReplayPlanState,
}

impl ReplayProjection {
    #[cfg(test)]
    pub(super) fn new(session_id: impl Into<String>) -> Self {
        Self {
            agent_id: None,
            session_id: session_id.into(),
        }
    }

    /// Selects the vendor adapter that may interpret replayed extension metadata.
    pub(super) fn for_agent(agent_id: impl Into<String>, session_id: impl Into<String>) -> Self {
        Self {
            agent_id: Some(agent_id.into()),
            session_id: session_id.into(),
        }
    }

    #[cfg(test)]
    pub(super) fn project(&self, updates: Vec<SessionUpdate>) -> Vec<NormalizedMessage> {
        self.project_with_plan(updates).messages
    }

    pub(super) fn project_with_plan(&self, updates: Vec<SessionUpdate>) -> ReplayProjectionResult {
        let created_at = now_string();
        let tool_calls = Arc::new(Mutex::new(Default::default()));
        let codex_subagents =
            (self.agent_id.as_deref() == Some("codex")).then(CodexSubagentState::default);
        let mut replay = ReplayBuffer::new(&self.session_id);
        for update in updates {
            replay.project(update, &created_at, &tool_calls, codex_subagents.as_ref());
        }
        replay.finalize_fallback_ids();
        ReplayProjectionResult {
            messages: replay.messages,
            plan: AgentReplayPlanState {
                current_plan: replay.current_plan,
                completed_plan_message_id: replay.completed_plan_message_id,
            },
        }
    }
}

struct ReplayBuffer {
    messages: Vec<NormalizedMessage>,
    // Anonymous chunks have no durable correlation and merge only within one contiguous run.
    active_anonymous_text: Option<ActiveAnonymousTextRun>,
    sourced_user_indices: HashMap<String, usize>,
    // ACP message ids keep each Agent or Thought channel open across interleaved updates.
    sourced_agent_indices: HashMap<(AgentMessageRole, String), usize>,
    session_id: String,
    next_text_ordinal: usize,
    current_plan: Option<AgentPlan>,
    completed_plan_message_id: Option<String>,
    next_plan_ordinal: usize,
}

impl ReplayBuffer {
    fn new(session_id: &str) -> Self {
        Self {
            messages: Vec::new(),
            active_anonymous_text: None,
            sourced_user_indices: HashMap::new(),
            sourced_agent_indices: HashMap::new(),
            session_id: session_id.to_string(),
            next_text_ordinal: 0,
            current_plan: None,
            completed_plan_message_id: None,
            next_plan_ordinal: 0,
        }
    }

    fn project(
        &mut self,
        update: SessionUpdate,
        created_at: &str,
        tool_calls: &ToolCallState,
        codex_subagents: Option<&CodexSubagentState>,
    ) {
        match update {
            SessionUpdate::UserMessageChunk(chunk) => {
                if let ContentBlock::Text(text) = chunk.content {
                    self.push_user_text(
                        text.text,
                        chunk.message_id.map(|id| id.to_string()),
                        created_at,
                    );
                }
            }
            SessionUpdate::AgentMessageChunk(chunk) => self.push_agent_part(
                AgentMessageRole::Agent,
                project_content_block(chunk.content, AgentMessageRole::Agent),
                chunk.message_id.map(|id| id.to_string()),
                created_at,
            ),
            SessionUpdate::AgentThoughtChunk(chunk) => self.push_agent_part(
                AgentMessageRole::Thought,
                project_content_block(chunk.content, AgentMessageRole::Thought),
                chunk.message_id.map(|id| id.to_string()),
                created_at,
            ),
            SessionUpdate::ToolCall(tool_call) => {
                self.end_anonymous_text_run();
                remember_tool_call(tool_calls, tool_call.clone());
                match codex_subagents
                    .map(|state| state.project_tool_call(&tool_call))
                    .unwrap_or(CodexSubagentProjection::GenericTool)
                {
                    CodexSubagentProjection::Event(subagent) => {
                        self.upsert(normalize_event(
                            crate::agent::events::AgentEvent::Subagent(subagent),
                            created_at,
                        ));
                    }
                    CodexSubagentProjection::Suppress => {}
                    CodexSubagentProjection::GenericTool => {
                        self.upsert(normalize_event(
                            replay_tool_event(&tool_call, codex_subagents.is_some()),
                            created_at,
                        ));
                    }
                }
            }
            SessionUpdate::ToolCallUpdate(update) => {
                self.end_anonymous_text_run();
                let subagent_projection = codex_subagents
                    .map(|state| state.project_tool_update(&update))
                    .unwrap_or(CodexSubagentProjection::GenericTool);
                let tool_call = merge_tool_call_update(tool_calls, update);
                match subagent_projection {
                    CodexSubagentProjection::Event(subagent) => {
                        self.upsert(normalize_event(
                            crate::agent::events::AgentEvent::Subagent(subagent),
                            created_at,
                        ));
                    }
                    CodexSubagentProjection::Suppress => {}
                    CodexSubagentProjection::GenericTool => {
                        self.upsert(normalize_event(
                            replay_tool_event(&tool_call, codex_subagents.is_some()),
                            created_at,
                        ));
                    }
                }
            }
            SessionUpdate::Plan(plan) => {
                self.end_anonymous_text_run();
                let Some(entries) = plan
                    .entries
                    .into_iter()
                    .map(project_plan_entry)
                    .collect::<Option<Vec<_>>>()
                else {
                    crate::logging::warn(
                        "acp_replayed_plan_update_ignored",
                        serde_json::json!({
                            "session_id": self.session_id,
                            "reason": "unsupported plan entry enum value",
                        }),
                    );
                    return;
                };
                self.replace_plan(entries, created_at);
            }
            _ => self.end_anonymous_text_run(),
        }
    }

    fn replace_plan(
        &mut self,
        entries: Vec<crate::protocol::model::AgentPlanEntry>,
        created_at: &str,
    ) {
        if entries.is_empty() {
            self.current_plan = None;
            self.completed_plan_message_id = None;
            return;
        }
        if entries
            .iter()
            .all(|entry| entry.status == AgentPlanStatus::Completed)
        {
            let id = self.completed_plan_message_id.clone().unwrap_or_else(|| {
                let id = format!(
                    "acp:{}:replay:plan:{}",
                    self.session_id, self.next_plan_ordinal
                );
                self.next_plan_ordinal += 1;
                id
            });
            self.upsert(NormalizedMessage::CompletedPlan {
                id: id.clone(),
                entries,
                created_at: created_at.to_string(),
            });
            self.current_plan = None;
            self.completed_plan_message_id = Some(id);
            return;
        }
        self.current_plan = Some(AgentPlan { entries });
        self.completed_plan_message_id = None;
    }

    fn push_agent_part(
        &mut self,
        role: AgentMessageRole,
        part: AgentMessagePart,
        source_message_id: Option<String>,
        created_at: &str,
    ) {
        let Some(source_message_id) = source_message_id else {
            if let AgentMessagePart::Text { text } = part {
                let kind = match role {
                    AgentMessageRole::Agent => ReplayTextKind::Agent,
                    AgentMessageRole::Thought => ReplayTextKind::Thought,
                };
                self.push_anonymous_text(kind, text, created_at);
            } else {
                self.end_anonymous_text_run();
                let id = format!(
                    "acp:{}:replay:{}:{}",
                    self.session_id,
                    role.label(),
                    self.next_text_ordinal
                );
                self.next_text_ordinal += 1;
                self.messages.push(NormalizedMessage::AgentMessage {
                    id,
                    role,
                    parts: vec![part],
                    created_at: created_at.to_string(),
                });
            }
            return;
        };

        self.end_anonymous_text_run();
        let source_key = (role, source_message_id.clone());
        if let Some(message_index) = self.sourced_agent_indices.get(&source_key).copied() {
            append_agent_part(&mut self.messages[message_index], role, part);
            return;
        }
        let message_index = self.messages.len();
        self.messages.push(NormalizedMessage::AgentMessage {
            id: stable_agent_message_id(&self.session_id, role, &source_message_id),
            role,
            parts: vec![part],
            created_at: created_at.to_string(),
        });
        self.sourced_agent_indices.insert(source_key, message_index);
    }

    fn push_user_text(
        &mut self,
        text: String,
        source_message_id: Option<String>,
        created_at: &str,
    ) {
        let Some(source_message_id) = source_message_id else {
            self.push_anonymous_text(ReplayTextKind::User, text, created_at);
            return;
        };

        self.end_anonymous_text_run();
        if let Some(message_index) = self.sourced_user_indices.get(&source_message_id).copied() {
            append_text_chunk(&mut self.messages[message_index], &text);
            return;
        }

        let message_index = self.messages.len();
        let message_id = stable_message_id(&self.session_id, &source_message_id);
        self.messages
            .push(ReplayTextKind::User.new_message(message_id, text, created_at));
        self.sourced_user_indices
            .insert(source_message_id, message_index);
    }

    fn push_anonymous_text(&mut self, kind: ReplayTextKind, text: String, created_at: &str) {
        if let Some(active) = self.active_anonymous_text.as_ref() {
            let message_index = active.message_index;
            if active.kind == kind {
                append_text_chunk(&mut self.messages[message_index], &text);
                return;
            }
        }

        let message_index = self.messages.len();
        let message_id = format!(
            "acp:{}:replay:{}:{}",
            self.session_id,
            kind.label(),
            self.next_text_ordinal
        );
        self.next_text_ordinal += 1;
        self.messages
            .push(kind.new_message(message_id, text, created_at));
        self.active_anonymous_text = Some(ActiveAnonymousTextRun {
            kind,
            message_index,
        });
    }

    fn upsert(&mut self, mut message: NormalizedMessage) {
        let identity = message.identity();
        if let Some(existing) = self
            .messages
            .iter_mut()
            .find(|existing| existing.identity() == identity)
        {
            message.preserve_created_at_from(existing);
            *existing = message;
        } else {
            self.messages.push(message);
        }
    }

    fn end_anonymous_text_run(&mut self) {
        self.active_anonymous_text = None;
    }

    fn finalize_fallback_ids(&mut self) {
        let mut occurrences = HashMap::<(ReplayTextKind, u64), usize>::new();
        for message in &mut self.messages {
            let Some((kind, text, id)) = replay_text_parts(message) else {
                continue;
            };
            if !id.contains(":replay:") {
                continue;
            }
            let fingerprint = stable_text_fingerprint(text);
            let occurrence = occurrences.entry((kind, fingerprint)).or_default();
            let stable_id = format!(
                "acp:{}:replay:{}:{fingerprint:016x}:{}",
                self.session_id,
                kind.label(),
                *occurrence
            );
            *occurrence += 1;
            set_message_id(message, stable_id);
        }
    }
}

fn replay_tool_event(
    tool_call: &crate::agent::acp_schema::ToolCall,
    codex: bool,
) -> crate::agent::events::AgentEvent {
    if codex {
        if let Some(tool) = project_codex_collaboration(tool_call) {
            return crate::agent::events::AgentEvent::ToolCall(tool);
        }
    }
    tool_call_event(tool_call)
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
enum ReplayTextKind {
    User,
    Agent,
    Thought,
}

impl ReplayTextKind {
    fn label(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Agent => "agent",
            Self::Thought => "thought",
        }
    }

    fn new_message(self, id: String, text: String, created_at: &str) -> NormalizedMessage {
        match self {
            Self::User => NormalizedMessage::User {
                id,
                text,
                created_at: created_at.to_string(),
                attachments: Vec::new(),
            },
            Self::Agent => NormalizedMessage::AgentMessage {
                id,
                role: AgentMessageRole::Agent,
                parts: vec![AgentMessagePart::Text { text }],
                created_at: created_at.to_string(),
            },
            Self::Thought => NormalizedMessage::AgentMessage {
                id,
                role: AgentMessageRole::Thought,
                parts: vec![AgentMessagePart::Text { text }],
                created_at: created_at.to_string(),
            },
        }
    }
}

fn set_message_id(message: &mut NormalizedMessage, message_id: String) {
    match message {
        NormalizedMessage::User { id, .. } | NormalizedMessage::AgentMessage { id, .. } => {
            *id = message_id
        }
        _ => {}
    }
}

fn replay_text_parts(message: &NormalizedMessage) -> Option<(ReplayTextKind, &str, &str)> {
    match message {
        NormalizedMessage::User { id, text, .. } => Some((ReplayTextKind::User, text, id)),
        NormalizedMessage::AgentMessage {
            id, role, parts, ..
        } => match parts.as_slice() {
            [AgentMessagePart::Text { text }] => Some((
                match role {
                    AgentMessageRole::Agent => ReplayTextKind::Agent,
                    AgentMessageRole::Thought => ReplayTextKind::Thought,
                },
                text,
                id,
            )),
            _ => None,
        },
        _ => None,
    }
}

fn stable_text_fingerprint(text: &str) -> u64 {
    // FNV-1a is deliberately fixed rather than process-randomized: replay identity must survive restarts.
    text.as_bytes()
        .iter()
        .fold(0xcbf29ce484222325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        })
}

#[derive(Clone)]
struct ActiveAnonymousTextRun {
    kind: ReplayTextKind,
    message_index: usize,
}

fn append_text_chunk(message: &mut NormalizedMessage, chunk: &str) {
    let text = match message {
        NormalizedMessage::User { text, .. } => text,
        NormalizedMessage::AgentMessage { parts, .. } => match parts.last_mut() {
            Some(AgentMessagePart::Text { text }) => text,
            _ => unreachable!("replay text index must reference a text message"),
        },
        _ => unreachable!("replay text index must reference a text message"),
    };
    text.push_str(chunk);
}

fn append_agent_part(
    message: &mut NormalizedMessage,
    role: AgentMessageRole,
    part: AgentMessagePart,
) {
    let NormalizedMessage::AgentMessage {
        role: existing_role,
        parts,
        ..
    } = message
    else {
        unreachable!("sourced ACP message index must reference an Agent message")
    };
    if *existing_role != role {
        return;
    }
    if let (Some(AgentMessagePart::Text { text }), AgentMessagePart::Text { text: chunk }) =
        (parts.last_mut(), &part)
    {
        text.push_str(chunk);
    } else {
        parts.push(part);
    }
}

impl AgentMessageRole {
    fn label(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Thought => "thought",
        }
    }
}
