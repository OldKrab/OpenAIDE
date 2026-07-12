use std::collections::BTreeMap;
use std::sync::Mutex;

use openaide_app_server_protocol::events::TextChunk;
use uuid::Uuid;

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::NormalizedMessage;

#[cfg(test)]
#[path = "streaming_tests.rs"]
mod tests;

const MAX_ACTIVE_STREAMS_PER_KIND: usize = 256;

#[derive(Default)]
pub(super) struct StreamingRuns {
    text: Mutex<TextRuns>,
    thought: Mutex<TextRuns>,
}

#[derive(Clone)]
pub(super) struct StreamingWrite {
    pub(super) message: NormalizedMessage,
    pub(super) delta: StreamingDelta,
    stream: StreamKind,
    run_key: RunKey,
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
    ) -> Result<StreamingWrite, RuntimeError> {
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
    ) -> Result<StreamingWrite, RuntimeError> {
        stream_chunk(
            &self.thought,
            text,
            source_message_id,
            now,
            MessageKind::Thought,
        )
    }

    pub(super) fn finish_text(&self, now: &str) -> Vec<StreamingWrite> {
        finish_runs(&self.text, now, MessageKind::AgentText)
    }

    pub(super) fn finish_thought(&self, now: &str) -> Vec<StreamingWrite> {
        finish_runs(&self.thought, now, MessageKind::Thought)
    }

    pub(super) fn finish_anonymous_text(&self, now: &str) -> Option<StreamingWrite> {
        finish_anonymous_run(&self.text, now, MessageKind::AgentText)
    }

    pub(super) fn finish_anonymous_thought(&self, now: &str) -> Option<StreamingWrite> {
        finish_anonymous_run(&self.thought, now, MessageKind::Thought)
    }

    pub(super) fn rollback(&self, write: StreamingWrite) {
        let slot = match write.stream {
            StreamKind::Text => &self.text,
            StreamKind::Thought => &self.thought,
        };
        let mut runs = slot.lock().expect("streaming run lock poisoned");
        match write.run_key {
            RunKey::Anonymous => runs.anonymous = write.previous,
            RunKey::Sourced(source_message_id) => match write.previous {
                Some(previous) => {
                    runs.sourced.insert(source_message_id, previous);
                }
                None => {
                    runs.sourced.remove(&source_message_id);
                }
            },
        }
    }
}

#[derive(Default)]
struct TextRuns {
    anonymous: Option<TextRun>,
    sourced: BTreeMap<String, TextRun>,
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

#[derive(Clone)]
enum RunKey {
    Anonymous,
    Sourced(String),
}

fn stream_chunk(
    slot: &Mutex<TextRuns>,
    text: String,
    source_message_id: Option<String>,
    now: &str,
    kind: MessageKind,
) -> Result<StreamingWrite, RuntimeError> {
    let mut runs = slot.lock().expect("streaming run lock poisoned");
    let run_key = source_message_id
        .as_ref()
        .map_or(RunKey::Anonymous, |id| RunKey::Sourced(id.clone()));
    let (run, previous) = match &run_key {
        RunKey::Anonymous => {
            let previous = runs.anonymous.clone();
            if previous.is_none() && active_stream_count(&runs) >= MAX_ACTIVE_STREAMS_PER_KIND {
                return Err(active_stream_limit_error());
            }
            let run = runs.anonymous.get_or_insert_with(|| new_text_run(None));
            (run, previous)
        }
        RunKey::Sourced(source_message_id) => {
            let previous = runs.sourced.get(source_message_id).cloned();
            if previous.is_none() {
                if let Some(mut anonymous) = runs.anonymous.take() {
                    anonymous.source_message_id = Some(source_message_id.clone());
                    runs.sourced.insert(source_message_id.clone(), anonymous);
                } else if active_stream_count(&runs) >= MAX_ACTIVE_STREAMS_PER_KIND {
                    return Err(active_stream_limit_error());
                }
            }
            let run = runs
                .sourced
                .entry(source_message_id.clone())
                .or_insert_with(|| new_text_run(Some(source_message_id.clone())));
            (run, previous)
        }
    };
    let first = previous.is_none() && run.text.is_empty();
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
    Ok(StreamingWrite {
        message: message(kind, run, now, true),
        delta,
        stream: stream_kind(kind),
        run_key,
        previous,
    })
}

fn active_stream_count(runs: &TextRuns) -> usize {
    runs.sourced.len() + usize::from(runs.anonymous.is_some())
}

fn active_stream_limit_error() -> RuntimeError {
    RuntimeError::Internal(format!(
        "active Agent message stream limit exceeded ({MAX_ACTIVE_STREAMS_PER_KIND})"
    ))
}

fn new_text_run(source_message_id: Option<String>) -> TextRun {
    TextRun {
        id: Uuid::new_v4().to_string(),
        source_message_id,
        text: String::new(),
        next_sequence: 1,
    }
}

fn finish_runs(slot: &Mutex<TextRuns>, now: &str, kind: MessageKind) -> Vec<StreamingWrite> {
    let mut runs = slot.lock().expect("streaming run lock poisoned");
    let mut writes = Vec::with_capacity(runs.sourced.len() + usize::from(runs.anonymous.is_some()));
    if let Some(run) = runs.anonymous.take() {
        writes.push(finish_write(run, RunKey::Anonymous, now, kind));
    }
    writes.extend(
        std::mem::take(&mut runs.sourced)
            .into_iter()
            .map(|(source_message_id, run)| {
                finish_write(run, RunKey::Sourced(source_message_id), now, kind)
            }),
    );
    writes
}

fn finish_write(run: TextRun, run_key: RunKey, now: &str, kind: MessageKind) -> StreamingWrite {
    StreamingWrite {
        message: message(kind, &run, now, false),
        delta: StreamingDelta::Chunk(TextChunk {
            sequence: run.next_sequence,
            text: String::new(),
            final_chunk: true,
        }),
        stream: stream_kind(kind),
        run_key,
        previous: Some(run),
    }
}

fn finish_anonymous_run(
    slot: &Mutex<TextRuns>,
    now: &str,
    kind: MessageKind,
) -> Option<StreamingWrite> {
    let run = slot
        .lock()
        .expect("streaming run lock poisoned")
        .anonymous
        .take()?;
    Some(finish_write(run, RunKey::Anonymous, now, kind))
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
