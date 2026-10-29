//! Task projection of Subagents announced in the released ACP form.

use crate::agent::events::{
    AgentEvent, AgentNativeSubagentCapabilities, AgentNativeSubagentIdentityUpdate,
    AgentNativeSubagentSpawned, AgentNativeSubagentState, AgentNativeSubagentStateUpdate,
};
use crate::agent::AgentSessionEventSink;
use crate::protocol::model::NormalizedMessage;
use crate::storage::subagents::SubagentStatusRecord;

use super::tests::{running_task, test_runtime};
use super::TaskSessionEventSink;

fn released_spawn(
    name: &str,
    delegated_task: Option<&str>,
    cancel: bool,
) -> AgentNativeSubagentSpawned {
    AgentNativeSubagentSpawned {
        parent_native_session_id: "session_1".to_string(),
        native_session_id: "child_1".to_string(),
        name: name.to_string(),
        delegated_task: delegated_task.map(str::to_string),
        parent_interaction: false,
        parent_records_delegation: true,
        capabilities: AgentNativeSubagentCapabilities {
            cancel,
            close: false,
        },
        details: Vec::new(),
    }
}

fn state(state: AgentNativeSubagentState) -> AgentNativeSubagentStateUpdate {
    AgentNativeSubagentStateUpdate {
        parent_native_session_id: "session_1".to_string(),
        native_session_id: "child_1".to_string(),
        state,
    }
}

#[test]
fn released_child_adds_no_delegated_row_and_is_reused_across_runs() {
    let (_dir, store, mutations, server_requests) = test_runtime();
    store.write_task(&running_task("task_1")).unwrap();
    let sink = TaskSessionEventSink::new(
        mutations,
        "task_1".to_string(),
        "session_1".to_string(),
        server_requests,
    );

    sink.subagent_spawned(released_spawn(
        "general-purpose",
        Some("Fix the build"),
        true,
    ))
    .unwrap();
    sink.subagent_state_changed(state(AgentNativeSubagentState::Running))
        .unwrap();
    for (message_id, text) in [("call_1", "Make the build pass"), ("call_2", "Try again")] {
        sink.subagent_session_update(
            "child_1",
            AgentEvent::UserMessageChunk {
                text: text.to_string(),
                source_message_id: Some(message_id.to_string()),
            },
        )
        .unwrap();
        if message_id == "call_1" {
            sink.subagent_state_changed(state(AgentNativeSubagentState::Completed))
                .unwrap();
            sink.subagent_state_changed(state(AgentNativeSubagentState::Running))
                .unwrap();
        }
    }

    // The parent's own Agent Tool row records the delegation in its Chat.
    assert!(store.read_messages("task_1").unwrap().is_empty());
    let record = store
        .subagent_record_by_native("task_1", "child_1")
        .unwrap();
    assert_eq!(record.status, SubagentStatusRecord::Running);
    assert_eq!(record.name, "general-purpose");
    assert_eq!(record.delegated_task, "Fix the build");
    assert!(record.capabilities.cancel);
    let history = store
        .subagent_history("task_1", &record.subagent_id)
        .unwrap();
    let prompts: Vec<_> = history
        .messages
        .iter()
        .filter_map(|stored| match &stored.chat.message {
            NormalizedMessage::User { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(prompts, ["Make the build pass", "Try again"]);
}

#[test]
fn released_identity_patches_and_reannouncement_update_the_stored_child() {
    let (_dir, store, mutations, server_requests) = test_runtime();
    store.write_task(&running_task("task_1")).unwrap();
    let sink = TaskSessionEventSink::new(
        mutations,
        "task_1".to_string(),
        "session_1".to_string(),
        server_requests,
    );
    sink.subagent_spawned(released_spawn("Explore", Some("Look around"), true))
        .unwrap();

    sink.subagent_identity_changed(AgentNativeSubagentIdentityUpdate {
        native_session_id: "child_1".to_string(),
        name: Some("reviewer (Explore)".to_string()),
        delegated_task: None,
        cancel: Some(false),
    })
    .unwrap();
    let record = store
        .subagent_record_by_native("task_1", "child_1")
        .unwrap();
    assert_eq!(record.name, "reviewer (Explore)");
    assert_eq!(record.delegated_task, "Look around");
    assert!(!record.capabilities.cancel);

    // After a resume, a fresh connection announces the stored child again.
    sink.subagent_spawned(released_spawn("Explore", None, true))
        .unwrap();
    let record = store
        .subagent_record_by_native("task_1", "child_1")
        .unwrap();
    assert_eq!(record.name, "Explore");
    assert_eq!(record.delegated_task, "");
    assert!(record.capabilities.cancel);
    assert_eq!(store.subagent_catalog("task_1").unwrap().entries.len(), 1);
}

#[test]
fn subagent_history_is_published_only_while_a_client_watches_it() {
    let dir = tempfile::tempdir().unwrap();
    let store = crate::storage::Store::open(dir.path().to_path_buf()).unwrap();
    let (notifier, updates) = crate::task_events::TaskUpdateNotifier::channel();
    let mutations = crate::tasks::mutation::TaskMutations::new(
        store.clone(),
        std::sync::Arc::new(std::sync::Mutex::new(())),
        std::sync::Arc::new(std::sync::Mutex::new(
            crate::tasks::runtime_state::RuntimeState::with_revision(0),
        )),
        notifier,
    );
    store.write_task(&running_task("task_1")).unwrap();
    let presence = crate::state_sync::TaskSubscriptionPresence::default();
    let sink = TaskSessionEventSink::new(
        mutations,
        "task_1".to_string(),
        "session_1".to_string(),
        crate::server_requests::ServerRequestRuntime::new(),
    )
    .with_subscription_presence(Some(presence.clone()));
    let prompt = |text: &str, message_id: &str| {
        sink.subagent_session_update(
            "child_1",
            AgentEvent::UserMessageChunk {
                text: text.to_string(),
                source_message_id: Some(message_id.to_string()),
            },
        )
        .unwrap();
    };
    // Counts (catalog, history) publications since the last call.
    let published = || {
        let (mut catalogs, mut histories) = (0, 0);
        while let Ok(update) = updates.try_recv() {
            match update.kind {
                crate::task_events::TaskUpdateKind::SubagentCatalogChanged { .. } => catalogs += 1,
                crate::task_events::TaskUpdateKind::SubagentHistoryChanged { .. } => histories += 1,
                _ => {}
            }
        }
        (catalogs, histories)
    };

    sink.subagent_spawned(released_spawn(
        "general-purpose",
        Some("Fix the build"),
        true,
    ))
    .unwrap();
    prompt("Make the build pass", "call_1");
    let (catalogs, histories) = published();
    assert!(catalogs > 0, "the Subagent list is always published");
    assert_eq!(histories, 0, "nobody watches the history yet");

    let subagent_id = store
        .subagent_record_by_native("task_1", "child_1")
        .unwrap()
        .subagent_id;
    presence.watch_subagent_history_for_test("task_1", &subagent_id);
    prompt("Try again", "call_2");
    let (_, histories) = published();
    assert!(histories > 0, "a watched history is published");
}
