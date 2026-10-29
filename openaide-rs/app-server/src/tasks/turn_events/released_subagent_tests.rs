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
    // The prompts are streamed writes, which commit after the sink returns.
    crate::test_sync::wait_until("both prompts in the child history", || {
        let history = store
            .subagent_history("task_1", &record.subagent_id)
            .unwrap();
        let prompts: Vec<_> = history
            .messages
            .iter()
            .filter_map(|stored| match &stored.chat.message {
                NormalizedMessage::User { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect();
        prompts == ["Make the build pass", "Try again"]
    });
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

/// A sink whose committed Subagent changes are published through the store.
fn publishing_sink() -> (
    tempfile::TempDir,
    crate::storage::Store,
    crate::task_events::TaskUpdateReceiver,
    TaskSessionEventSink,
) {
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
    let sink = TaskSessionEventSink::new(
        mutations,
        "task_1".to_string(),
        "session_1".to_string(),
        crate::server_requests::ServerRequestRuntime::new(),
    );
    (dir, store, updates, sink)
}

#[test]
fn a_subagent_history_is_published_as_one_change_per_revision() {
    use crate::protocol::model::{
        AgentMessagePart, AgentMessageRole, AgentPlan, AgentPlanEntry, AgentPlanPriority,
        AgentPlanStatus,
    };
    use crate::task_events::TaskUpdateKind;
    use openaide_app_server_protocol::events::TaskChatChange;

    let (_dir, store, updates, sink) = publishing_sink();
    sink.subagent_spawned(released_spawn("general-purpose", None, true))
        .unwrap();
    let subagent_id = store
        .subagent_record_by_native("task_1", "child_1")
        .unwrap()
        .subagent_id;
    let update = |event: AgentEvent| sink.subagent_session_update("child_1", event).unwrap();
    let agent_text = |text: &str| AgentEvent::MessageChunk {
        role: AgentMessageRole::Agent,
        part: AgentMessagePart::Text {
            text: text.to_string(),
        },
        source_message_id: Some("reply".to_string()),
    };
    let plan = |entries: Vec<AgentPlanEntry>| AgentEvent::Plan(AgentPlan { entries });

    update(AgentEvent::UserMessageChunk {
        text: "Fix".to_string(),
        source_message_id: Some("prompt".to_string()),
    });
    update(AgentEvent::UserMessageChunk {
        text: " it".to_string(),
        source_message_id: Some("prompt".to_string()),
    });
    update(agent_text("Done"));
    update(agent_text(" now"));
    update(plan(vec![AgentPlanEntry {
        content: "Run the tests".to_string(),
        priority: AgentPlanPriority::High,
        status: AgentPlanStatus::Pending,
    }]));
    update(plan(Vec::new()));

    // Streamed writes commit and publish after the sink returns.
    let mut changes = Vec::new();
    let mut catalog_history_revision = 0;
    crate::test_sync::wait_until("every Subagent history revision", || {
        let Ok(update) = updates.try_recv() else {
            return changes.len() >= 6 && catalog_history_revision == 6;
        };
        if let TaskUpdateKind::SubagentCatalogChanged { catalog } = &update.kind {
            catalog_history_revision = catalog.entries[0].history_revision;
        }
        if let TaskUpdateKind::SubagentHistoryChanged {
            subagent_id: changed,
            chat,
            current_plan,
        } = update.kind
        {
            assert_eq!(changed.as_str(), subagent_id);
            let kind = match (&chat[..], &current_plan) {
                ([TaskChatChange::Append { .. }], None) => "append",
                ([TaskChatChange::Upsert { .. }], None) => "upsert",
                ([TaskChatChange::AppendText { text, .. }], None) if text == " now" => "appendText",
                ([], Some(Some(_))) => "setPlan",
                ([], Some(None)) => "clearPlan",
                other => panic!("unexpected Subagent history change {other:?}"),
            };
            changes.push((update.revision, kind));
        }
        false
    });
    assert_eq!(
        changes,
        [
            (1, "append"),
            (2, "upsert"),
            (3, "append"),
            (4, "appendText"),
            (5, "setPlan"),
            (6, "clearPlan"),
        ]
    );
    assert_eq!(
        store
            .subagent_history("task_1", &subagent_id)
            .unwrap()
            .revision,
        6,
        "every committed revision was published"
    );
}
