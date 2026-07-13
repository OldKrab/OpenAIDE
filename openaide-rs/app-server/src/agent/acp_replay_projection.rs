use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use agent_client_protocol::schema::{ContentBlock, SessionUpdate};

use crate::agent::acp_content_projection::non_text_content_event;
use crate::agent::acp_tool_call_projection::{
    merge_tool_call_update, remember_tool_call, ToolCallState,
};
use crate::agent::normalizer::normalize_event;
use crate::agent::tool_details::tool_call_event;
use crate::protocol::model::{AgentContentRole, NormalizedMessage};
use crate::time::now_string;

pub(super) struct ReplayProjection {
    session_id: String,
}

impl ReplayProjection {
    pub(super) fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
        }
    }

    pub(super) fn project(&self, updates: Vec<SessionUpdate>) -> Vec<NormalizedMessage> {
        let created_at = now_string();
        let tool_calls = Arc::new(Mutex::new(Default::default()));
        let mut replay = ReplayBuffer::new(&self.session_id);
        for update in updates {
            replay.project(update, &created_at, &tool_calls);
        }
        replay.finalize_fallback_ids();
        replay.messages
    }
}

struct ReplayBuffer {
    messages: Vec<NormalizedMessage>,
    // Anonymous chunks have no durable correlation and merge only within one contiguous run.
    active_anonymous_text: Option<ActiveAnonymousTextRun>,
    // Source ids keep their logical messages open across interleaved replay updates.
    sourced_text_indices: HashMap<(ReplayTextKind, String), usize>,
    session_id: String,
    next_text_ordinal: usize,
}

impl ReplayBuffer {
    fn new(session_id: &str) -> Self {
        Self {
            messages: Vec::new(),
            active_anonymous_text: None,
            sourced_text_indices: HashMap::new(),
            session_id: session_id.to_string(),
            next_text_ordinal: 0,
        }
    }

    fn project(&mut self, update: SessionUpdate, created_at: &str, tool_calls: &ToolCallState) {
        match update {
            SessionUpdate::UserMessageChunk(chunk) => {
                if let ContentBlock::Text(text) = chunk.content {
                    self.push_text(
                        ReplayTextKind::User,
                        text.text,
                        chunk.message_id,
                        created_at,
                    );
                }
            }
            SessionUpdate::AgentMessageChunk(chunk) => match chunk.content {
                ContentBlock::Text(text) => self.push_text(
                    ReplayTextKind::Agent,
                    text.text,
                    chunk.message_id,
                    created_at,
                ),
                content => self.push_content(
                    content,
                    AgentContentRole::Agent,
                    chunk.message_id,
                    created_at,
                ),
            },
            SessionUpdate::AgentThoughtChunk(chunk) => match chunk.content {
                ContentBlock::Text(text) => self.push_text(
                    ReplayTextKind::Thought,
                    text.text,
                    chunk.message_id,
                    created_at,
                ),
                content => self.push_content(
                    content,
                    AgentContentRole::Thought,
                    chunk.message_id,
                    created_at,
                ),
            },
            SessionUpdate::ToolCall(tool_call) => {
                self.end_anonymous_text_run();
                remember_tool_call(tool_calls, tool_call.clone());
                self.upsert(normalize_event(tool_call_event(&tool_call), created_at));
            }
            SessionUpdate::ToolCallUpdate(update) => {
                self.end_anonymous_text_run();
                let tool_call = merge_tool_call_update(tool_calls, update);
                self.upsert(normalize_event(tool_call_event(&tool_call), created_at));
            }
            _ => self.end_anonymous_text_run(),
        }
    }

    fn push_content(
        &mut self,
        content: ContentBlock,
        role: AgentContentRole,
        source_message_id: Option<String>,
        created_at: &str,
    ) {
        self.end_anonymous_text_run();
        if let Some(event) = non_text_content_event(content, role, source_message_id) {
            self.messages.push(normalize_event(event, created_at));
        }
    }

    fn push_text(
        &mut self,
        kind: ReplayTextKind,
        text: String,
        source_message_id: Option<String>,
        created_at: &str,
    ) {
        let Some(source_message_id) = source_message_id else {
            self.push_anonymous_text(kind, text, created_at);
            return;
        };

        self.end_anonymous_text_run();
        let source_key = (kind, source_message_id);
        if let Some(message_index) = self.sourced_text_indices.get(&source_key).copied() {
            append_text_chunk(&mut self.messages[message_index], &text);
            return;
        }

        let message_index = self.messages.len();
        let message_id = stable_source_id(&self.session_id, &source_key.1);
        self.messages
            .push(kind.new_message(message_id, text, created_at));
        self.sourced_text_indices.insert(source_key, message_index);
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
            Self::Agent => NormalizedMessage::AgentText {
                id,
                text,
                created_at: created_at.to_string(),
                streaming: false,
            },
            Self::Thought => NormalizedMessage::Thought {
                id,
                text,
                created_at: created_at.to_string(),
                streaming: false,
            },
        }
    }
}

fn stable_source_id(session_id: &str, source_message_id: &str) -> String {
    format!("acp:{session_id}:message:{source_message_id}")
}

fn set_message_id(message: &mut NormalizedMessage, message_id: String) {
    match message {
        NormalizedMessage::User { id, .. }
        | NormalizedMessage::AgentText { id, .. }
        | NormalizedMessage::Thought { id, .. } => *id = message_id,
        _ => {}
    }
}

fn replay_text_parts(message: &NormalizedMessage) -> Option<(ReplayTextKind, &str, &str)> {
    match message {
        NormalizedMessage::User { id, text, .. } => Some((ReplayTextKind::User, text, id)),
        NormalizedMessage::AgentText { id, text, .. } => Some((ReplayTextKind::Agent, text, id)),
        NormalizedMessage::Thought { id, text, .. } => Some((ReplayTextKind::Thought, text, id)),
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
        NormalizedMessage::User { text, .. }
        | NormalizedMessage::AgentText { text, .. }
        | NormalizedMessage::Thought { text, .. } => text,
        _ => unreachable!("replay text index must reference a text message"),
    };
    text.push_str(chunk);
}
