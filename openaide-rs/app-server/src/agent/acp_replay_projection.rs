use std::sync::{Arc, Mutex};

use agent_client_protocol::schema::{ContentBlock, SessionUpdate};

use crate::agent::acp_tool_call_projection::{
    merge_tool_call_update, remember_tool_call, ToolCallState,
};
use crate::agent::normalizer::normalize_event;
use crate::agent::tool_details::tool_call_event;
use crate::protocol::model::NormalizedMessage;
use crate::time::now_string;

pub(super) struct ReplayProjection;

impl ReplayProjection {
    pub(super) fn new() -> Self {
        Self
    }

    pub(super) fn project(&self, updates: Vec<SessionUpdate>) -> Vec<NormalizedMessage> {
        let created_at = now_string();
        let tool_calls = Arc::new(Mutex::new(Default::default()));
        let mut replay = ReplayBuffer::default();
        for update in updates {
            replay.project(update, &created_at, &tool_calls);
        }
        replay.messages
    }
}

#[derive(Default)]
struct ReplayBuffer {
    messages: Vec<NormalizedMessage>,
    active_text: Option<ActiveTextRun>,
}

impl ReplayBuffer {
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
            SessionUpdate::AgentMessageChunk(chunk) => {
                if let ContentBlock::Text(text) = chunk.content {
                    self.push_text(
                        ReplayTextKind::Agent,
                        text.text,
                        chunk.message_id,
                        created_at,
                    );
                }
            }
            SessionUpdate::AgentThoughtChunk(chunk) => {
                if let ContentBlock::Text(text) = chunk.content {
                    self.push_text(
                        ReplayTextKind::Thought,
                        text.text,
                        chunk.message_id,
                        created_at,
                    );
                }
            }
            SessionUpdate::ToolCall(tool_call) => {
                self.end_text_run();
                remember_tool_call(tool_calls, tool_call.clone());
                self.upsert(normalize_event(tool_call_event(&tool_call), created_at));
            }
            SessionUpdate::ToolCallUpdate(update) => {
                self.end_text_run();
                let tool_call = merge_tool_call_update(tool_calls, update);
                self.upsert(normalize_event(tool_call_event(&tool_call), created_at));
            }
            _ => self.end_text_run(),
        }
    }

    fn push_text(
        &mut self,
        kind: ReplayTextKind,
        text: String,
        source_message_id: Option<String>,
        created_at: &str,
    ) {
        if let Some(active) = self.active_text.as_ref() {
            let message_index = active.message_index;
            if active.kind == kind
                && source_message_ids_match(
                    active.source_message_id.as_deref(),
                    source_message_id.as_deref(),
                )
                && append_text_chunk(&mut self.messages[message_index], &text)
            {
                if active.source_message_id.is_none() {
                    self.active_text
                        .as_mut()
                        .expect("active replay text run")
                        .source_message_id = source_message_id;
                }
                return;
            }
        }

        let message_index = self.messages.len();
        self.messages.push(kind.new_message(text, created_at));
        self.active_text = Some(ActiveTextRun {
            kind,
            message_index,
            source_message_id,
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

    fn end_text_run(&mut self) {
        self.active_text = None;
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReplayTextKind {
    User,
    Agent,
    Thought,
}

impl ReplayTextKind {
    fn new_message(self, text: String, created_at: &str) -> NormalizedMessage {
        let id = uuid::Uuid::new_v4().to_string();
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

#[derive(Clone)]
struct ActiveTextRun {
    kind: ReplayTextKind,
    message_index: usize,
    source_message_id: Option<String>,
}

fn source_message_ids_match(current: Option<&str>, incoming: Option<&str>) -> bool {
    !matches!((current, incoming), (Some(current), Some(incoming)) if current != incoming)
}

fn append_text_chunk(message: &mut NormalizedMessage, chunk: &str) -> bool {
    let text = match message {
        NormalizedMessage::User { text, .. }
        | NormalizedMessage::AgentText { text, .. }
        | NormalizedMessage::Thought { text, .. } => text,
        _ => return false,
    };
    text.push_str(chunk);
    true
}
