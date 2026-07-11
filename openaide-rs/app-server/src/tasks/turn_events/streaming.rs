use std::sync::Mutex;

use openaide_app_server_protocol::events::TextChunk;
use uuid::Uuid;

use crate::protocol::model::NormalizedMessage;

#[derive(Default)]
pub(super) struct StreamingRuns {
    text: Mutex<Option<TextRun>>,
    thought: Mutex<Option<TextRun>>,
}

#[derive(Clone)]
pub(super) struct StreamingWrite {
    pub(super) message: NormalizedMessage,
    pub(super) delta: StreamingDelta,
    stream: StreamKind,
    previous: Option<TextRun>,
}

#[derive(Clone)]
pub(super) enum StreamingDelta {
    Append,
    Chunk(TextChunk),
}

impl StreamingRuns {
    pub(super) fn agent_text_chunk(
        &self,
        text: String,
        source_message_id: Option<String>,
        now: &str,
    ) -> StreamingWrite {
        stream_chunk(
            &self.text,
            text,
            source_message_id,
            now,
            MessageKind::AgentText,
        )
    }

    pub(super) fn thought_chunk(
        &self,
        text: String,
        source_message_id: Option<String>,
        now: &str,
    ) -> StreamingWrite {
        stream_chunk(
            &self.thought,
            text,
            source_message_id,
            now,
            MessageKind::Thought,
        )
    }

    pub(super) fn finish_text_for_source_change(
        &self,
        source_message_id: Option<&str>,
        now: &str,
    ) -> Option<StreamingWrite> {
        finish_for_source_change(&self.text, source_message_id, now, MessageKind::AgentText)
    }

    pub(super) fn finish_thought_for_source_change(
        &self,
        source_message_id: Option<&str>,
        now: &str,
    ) -> Option<StreamingWrite> {
        finish_for_source_change(&self.thought, source_message_id, now, MessageKind::Thought)
    }

    pub(super) fn finish_text(&self, now: &str) -> Option<StreamingWrite> {
        finish_run(&self.text, now, MessageKind::AgentText)
    }

    pub(super) fn finish_thought(&self, now: &str) -> Option<StreamingWrite> {
        finish_run(&self.thought, now, MessageKind::Thought)
    }

    pub(super) fn rollback(&self, write: StreamingWrite) {
        let slot = match write.stream {
            StreamKind::Text => &self.text,
            StreamKind::Thought => &self.thought,
        };
        *slot.lock().expect("streaming run lock poisoned") = write.previous;
    }
}

#[derive(Clone)]
struct TextRun {
    id: String,
    source_message_id: Option<String>,
    text: String,
    next_sequence: u64,
}

#[derive(Clone, Copy)]
enum MessageKind {
    AgentText,
    Thought,
}

#[derive(Clone, Copy)]
enum StreamKind {
    Text,
    Thought,
}

fn stream_chunk(
    slot: &Mutex<Option<TextRun>>,
    text: String,
    source_message_id: Option<String>,
    now: &str,
    kind: MessageKind,
) -> StreamingWrite {
    let mut slot = slot.lock().expect("streaming run lock poisoned");
    let previous = slot.clone();
    let first = slot.is_none();
    let run = slot.get_or_insert_with(|| TextRun {
        id: Uuid::new_v4().to_string(),
        source_message_id: source_message_id.clone(),
        text: String::new(),
        next_sequence: 1,
    });
    if run.source_message_id.is_none() {
        run.source_message_id = source_message_id;
    }
    run.text.push_str(&text);
    let delta = if first {
        StreamingDelta::Append
    } else {
        let sequence = run.next_sequence;
        run.next_sequence += 1;
        StreamingDelta::Chunk(TextChunk {
            sequence,
            text,
            final_chunk: false,
        })
    };
    StreamingWrite {
        message: message(kind, run, now, true),
        delta,
        stream: stream_kind(kind),
        previous,
    }
}

fn finish_for_source_change(
    slot: &Mutex<Option<TextRun>>,
    source_message_id: Option<&str>,
    now: &str,
    kind: MessageKind,
) -> Option<StreamingWrite> {
    let changed = {
        let slot = slot.lock().expect("streaming run lock poisoned");
        matches!(
            (slot.as_ref().and_then(|run| run.source_message_id.as_deref()), source_message_id),
            (Some(current), Some(incoming)) if current != incoming
        )
    };
    changed.then(|| finish_run(slot, now, kind)).flatten()
}

fn finish_run(
    slot: &Mutex<Option<TextRun>>,
    now: &str,
    kind: MessageKind,
) -> Option<StreamingWrite> {
    let run = slot.lock().expect("streaming run lock poisoned").take()?;
    Some(StreamingWrite {
        message: message(kind, &run, now, false),
        delta: StreamingDelta::Chunk(TextChunk {
            sequence: run.next_sequence,
            text: String::new(),
            final_chunk: true,
        }),
        stream: stream_kind(kind),
        previous: Some(run),
    })
}

fn stream_kind(kind: MessageKind) -> StreamKind {
    match kind {
        MessageKind::AgentText => StreamKind::Text,
        MessageKind::Thought => StreamKind::Thought,
    }
}

fn message(kind: MessageKind, run: &TextRun, now: &str, streaming: bool) -> NormalizedMessage {
    match kind {
        MessageKind::AgentText => NormalizedMessage::AgentText {
            id: run.id.clone(),
            text: run.text.clone(),
            created_at: now.to_string(),
            streaming,
        },
        MessageKind::Thought => NormalizedMessage::Thought {
            id: run.id.clone(),
            text: run.text.clone(),
            created_at: now.to_string(),
            streaming,
        },
    }
}
