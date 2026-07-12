use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;

use openaide_app_server_protocol::events::TextChunk;
use uuid::Uuid;

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::NormalizedMessage;

#[cfg(test)]
#[path = "streaming_tests.rs"]
mod tests;

// Agents overwhelmingly update recent messages. Bounding this working set keeps
// a pathological Turn from retaining every sourced message until completion.
const MAX_ACTIVE_STREAMS_PER_KIND: usize = 32;

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
    ) -> Result<Vec<StreamingWrite>, RuntimeError> {
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
    ) -> Result<Vec<StreamingWrite>, RuntimeError> {
        stream_chunk(
            &self.thought,
            text,
            source_message_id,
            now,
            MessageKind::Thought,
        )
    }

    #[cfg(test)]
    pub(super) fn finish_text(&self, now: &str) -> Vec<StreamingWrite> {
        finish_runs(&self.text, now, MessageKind::AgentText)
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
                    touch_recency(&mut runs.sourced_recency, &source_message_id);
                    runs.sourced.insert(source_message_id, previous);
                }
                None => {
                    runs.sourced.remove(&source_message_id);
                    if let Some(index) = runs
                        .sourced_recency
                        .iter()
                        .position(|id| id == &source_message_id)
                    {
                        runs.sourced_recency.remove(index);
                    }
                }
            },
        }
    }
}

#[derive(Default)]
struct TextRuns {
    anonymous: Option<TextRun>,
    sourced: BTreeMap<String, TextRun>,
    sourced_recency: VecDeque<String>,
}

#[derive(Clone)]
struct TextRun {
    id: String,
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
) -> Result<Vec<StreamingWrite>, RuntimeError> {
    let mut runs = slot.lock().expect("streaming run lock poisoned");
    let run_key = source_message_id
        .as_ref()
        .map_or(RunKey::Anonymous, |id| RunKey::Sourced(id.clone()));
    let mut finalized = Vec::new();
    let (run, previous) = match &run_key {
        RunKey::Anonymous => {
            let previous = runs.anonymous.clone();
            if previous.is_none() && active_stream_count(&runs) >= MAX_ACTIVE_STREAMS_PER_KIND {
                finalized.push(evict_oldest_sourced(&mut runs, now, kind)?);
            }
            let run = runs.anonymous.get_or_insert_with(new_text_run);
            (run, previous)
        }
        RunKey::Sourced(source_message_id) => {
            let previous = runs.sourced.get(source_message_id).cloned();
            if previous.is_none() {
                // Anonymous and sourced chunks have no shared protocol identity.
                // Preserve them as distinct Chat rows instead of guessing that a
                // later source id retroactively names the anonymous run.
                if let Some(anonymous) = runs.anonymous.take() {
                    finalized.push(finish_write(anonymous, RunKey::Anonymous, now, kind));
                }
                if active_stream_count(&runs) >= MAX_ACTIVE_STREAMS_PER_KIND {
                    finalized.push(evict_oldest_sourced(&mut runs, now, kind)?);
                }
            }
            touch_recency(&mut runs.sourced_recency, source_message_id);
            let run = runs
                .sourced
                .entry(source_message_id.clone())
                .or_insert_with(new_text_run);
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
    finalized.push(StreamingWrite {
        message: message(kind, run, now, true),
        delta,
        stream: stream_kind(kind),
        run_key,
        previous,
    });
    Ok(finalized)
}

fn active_stream_count(runs: &TextRuns) -> usize {
    runs.sourced.len() + usize::from(runs.anonymous.is_some())
}

fn evict_oldest_sourced(
    runs: &mut TextRuns,
    now: &str,
    kind: MessageKind,
) -> Result<StreamingWrite, RuntimeError> {
    let source_message_id = runs.sourced_recency.pop_front().ok_or_else(|| {
        RuntimeError::Internal("active Agent message streams have no evictable source".to_string())
    })?;
    let run = runs.sourced.remove(&source_message_id).ok_or_else(|| {
        RuntimeError::Internal("active Agent message recency is inconsistent".to_string())
    })?;
    Ok(finish_write(
        run,
        RunKey::Sourced(source_message_id),
        now,
        kind,
    ))
}

fn touch_recency(recency: &mut VecDeque<String>, source_message_id: &str) {
    if let Some(index) = recency.iter().position(|id| id == source_message_id) {
        recency.remove(index);
    }
    recency.push_back(source_message_id.to_string());
}

fn new_text_run() -> TextRun {
    TextRun {
        id: Uuid::new_v4().to_string(),
        text: String::new(),
        next_sequence: 1,
    }
}

#[cfg(test)]
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
    runs.sourced_recency.clear();
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
