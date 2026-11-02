use super::tests::{running_task, test_runtime};
use super::TaskSessionEventSink;
use crate::agent::events::{
    AgentEvent, AgentNativeSubagentCapabilities, AgentNativeSubagentSpawned,
    AgentNativeSubagentState, AgentNativeSubagentStateUpdate, AgentReplayedSubagentEvent,
};
use crate::agent::AgentSessionEventSink;
use crate::protocol::model::NormalizedMessage;
use crate::storage::subagents::SubagentStatusRecord;

fn spawned(child: &str) -> AgentReplayedSubagentEvent {
    AgentReplayedSubagentEvent::Spawned(AgentNativeSubagentSpawned {
        parent_native_session_id: "session_1".to_string(),
        native_session_id: child.to_string(),
        name: "Reviewer".to_string(),
        delegated_task: Some("Summary".to_string()),
        capabilities: AgentNativeSubagentCapabilities::default(),
        details: Vec::new(),
    })
}

fn prompt(child: &str, text: &str) -> AgentReplayedSubagentEvent {
    AgentReplayedSubagentEvent::Update {
        native_session_id: child.to_string(),
        event: AgentEvent::UserMessageChunk {
            text: text.to_string(),
            source_message_id: None,
        },
    }
}

fn state(child: &str, state: AgentNativeSubagentState) -> AgentReplayedSubagentEvent {
    AgentReplayedSubagentEvent::State(AgentNativeSubagentStateUpdate {
        parent_native_session_id: "session_1".to_string(),
        native_session_id: child.to_string(),
        state,
    })
}

#[test]
fn a_replayed_child_is_restored_with_its_history_and_outcome() {
    let (_dir, store, mutations, server_requests) = test_runtime();
    store.write_task(&running_task("task_1")).unwrap();
    let sink = TaskSessionEventSink::new(
        mutations,
        "task_1".to_string(),
        "session_1".to_string(),
        server_requests,
    );

    sink.restore_replayed_subagents(vec![
        spawned("child_1"),
        state("child_1", AgentNativeSubagentState::Running),
        prompt("child_1", "Review it"),
        state("child_1", AgentNativeSubagentState::Completed),
    ]);

    let record = store
        .subagent_record_by_native("task_1", "child_1")
        .unwrap();
    assert_eq!(record.status, SubagentStatusRecord::Completed);
    let history = store
        .subagent_history("task_1", &record.subagent_id)
        .unwrap();
    assert!(matches!(
        history.messages.as_slice(),
        [stored] if matches!(
            &stored.chat.message,
            NormalizedMessage::User { text, .. } if text == "Review it"
        )
    ));
}

/// ADR-0048: an outcome the load leaves unknown is Disconnected, not Running.
#[test]
fn a_replay_that_ends_without_an_outcome_leaves_the_child_disconnected() {
    let (_dir, store, mutations, server_requests) = test_runtime();
    store.write_task(&running_task("task_1")).unwrap();
    let sink = TaskSessionEventSink::new(
        mutations,
        "task_1".to_string(),
        "session_1".to_string(),
        server_requests,
    );

    sink.restore_replayed_subagents(vec![
        spawned("child_1"),
        state("child_1", AgentNativeSubagentState::Running),
    ]);

    assert_eq!(
        store
            .subagent_record_by_native("task_1", "child_1")
            .unwrap()
            .status,
        SubagentStatusRecord::Disconnected
    );
}

#[test]
fn a_stored_child_keeps_its_saved_history() {
    let (_dir, store, mutations, server_requests) = test_runtime();
    store.write_task(&running_task("task_1")).unwrap();
    let sink = TaskSessionEventSink::new(
        mutations,
        "task_1".to_string(),
        "session_1".to_string(),
        server_requests,
    );
    let AgentReplayedSubagentEvent::Spawned(saved) = spawned("child_1") else {
        unreachable!();
    };
    sink.subagent_spawned(saved).unwrap();
    sink.subagent_session_update(
        "child_1",
        AgentEvent::UserMessageChunk {
            text: "Saved prompt".to_string(),
            source_message_id: None,
        },
    )
    .unwrap();

    sink.restore_replayed_subagents(vec![
        spawned("child_1"),
        prompt("child_1", "Replayed prompt"),
        state("child_1", AgentNativeSubagentState::Failed),
        spawned("child_2"),
        prompt("child_2", "New prompt"),
        state("child_2", AgentNativeSubagentState::Completed),
    ]);

    let saved = store
        .subagent_record_by_native("task_1", "child_1")
        .unwrap();
    assert_ne!(saved.status, SubagentStatusRecord::Failed);
    let history = store
        .subagent_history("task_1", &saved.subagent_id)
        .unwrap();
    assert!(matches!(
        history.messages.as_slice(),
        [stored] if matches!(
            &stored.chat.message,
            NormalizedMessage::User { text, .. } if text == "Saved prompt"
        )
    ));
    assert_eq!(
        store
            .subagent_record_by_native("task_1", "child_2")
            .unwrap()
            .status,
        SubagentStatusRecord::Completed
    );
}
