//! Subagent History and Tool detail subscription replies by known version (ADR-0086).

use openaide_app_server_protocol::ids::{ClientInstanceId, SubagentId, TaskId};
use openaide_app_server_protocol::snapshot::SubagentHistoryAvailability;
use openaide_app_server_protocol::state::{SubagentChatSync, ToolTerminalSync};
use tempfile::TempDir;

use super::*;
use crate::protocol::model::{
    ActivityToolContent, ActivityToolDetails, AgentMessagePart, AgentMessageRole, NormalizedMessage,
};
use crate::storage::subagents::SubagentCapabilitiesRecord;
use crate::storage::task_journal::{TaskWrite, ToolArtifactReplacement, ToolTerminalAppend};
use crate::storage::Store;

const TASK: &str = "task-entities";
const ARTIFACT: &str = "artifact_1";

fn client() -> ClientInstanceId {
    ClientInstanceId::from("client-1")
}

fn open_store(root: &TempDir) -> Store {
    let store = Store::open(root.path().to_path_buf()).unwrap();
    let mut task = super::tests::task_record(TASK);
    task.lifecycle = crate::storage::records::TaskLifecycle::Open;
    store.write_task(&task).unwrap();
    store
}

fn details(text: &str) -> ActivityToolDetails {
    ActivityToolDetails {
        locations: Vec::new(),
        content: vec![ActivityToolContent::Text {
            text: text.to_string(),
        }],
        input: None,
        output: None,
    }
}

/// Commits one artifact frame: the structured details when given, then terminal appends.
fn commit_artifact(store: &Store, replace: Option<&str>, appends: &[(&str, &str)]) {
    store
        .task_journal()
        .submit(TaskWrite::barrier_operations_with_artifacts(
            TASK.to_string(),
            Vec::new(),
            replace
                .map(|text| ToolArtifactReplacement {
                    artifact_id: ARTIFACT.to_string(),
                    details: details(text),
                })
                .into_iter()
                .collect(),
            appends
                .iter()
                .map(|(terminal, data)| ToolTerminalAppend {
                    artifact_id: ARTIFACT.to_string(),
                    terminal_id: terminal.to_string(),
                    data: data.to_string(),
                })
                .collect(),
        ))
        .unwrap()
        .wait()
        .unwrap();
}

fn tool_detail(store: &Store, known: Option<u64>) -> crate::snapshots::ToolDetailSubscription {
    TaskSnapshotStore::new(store.clone())
        .tool_detail_subscription(&client(), &TaskId::from(TASK), ARTIFACT, known)
        .unwrap()
}

fn output_of(reply: &crate::snapshots::ToolDetailSubscription, terminal: &str) -> Option<String> {
    reply
        .details
        .terminal_outputs
        .iter()
        .find(|output| output.terminal_id == terminal)
        .map(|output| output.output.clone())
}

#[test]
fn a_tool_detail_without_a_known_version_replies_with_all_its_terminal_output() {
    let root = TempDir::new().unwrap();
    let store = open_store(&root);
    commit_artifact(&store, Some("first"), &[("terminal_1", "one")]);
    commit_artifact(&store, None, &[("terminal_1", "two")]);

    let reply = tool_detail(&store, None);

    assert_eq!(reply.details.revision, 2);
    assert!(matches!(reply.terminal_sync, ToolTerminalSync::Full));
    assert_eq!(output_of(&reply, "terminal_1").as_deref(), Some("onetwo"));
}

#[test]
fn the_current_tool_detail_version_replies_with_fields_and_no_output() {
    let root = TempDir::new().unwrap();
    let store = open_store(&root);
    commit_artifact(&store, Some("first"), &[("terminal_1", "one")]);

    let reply = tool_detail(&store, Some(1));

    assert!(matches!(reply.terminal_sync, ToolTerminalSync::Current));
    assert_eq!(reply.details.revision, 1);
    assert_eq!(reply.details.content.len(), 1, "fields are always complete");
    assert!(reply.details.terminal_outputs.is_empty());
}

#[test]
fn an_older_tool_detail_version_replies_with_the_appends_after_it_and_current_fields() {
    let root = TempDir::new().unwrap();
    let store = open_store(&root);
    commit_artifact(&store, Some("first"), &[("terminal_1", "one")]);
    commit_artifact(
        &store,
        Some("second"),
        &[("terminal_1", "two"), ("terminal_2", "x")],
    );
    commit_artifact(&store, None, &[("terminal_1", "three")]);

    let reply = tool_detail(&store, Some(1));

    assert_eq!(reply.details.revision, 3);
    let ToolTerminalSync::Deltas { appends } = &reply.terminal_sync else {
        panic!("expected deltas, got {:?}", reply.terminal_sync);
    };
    let appended: Vec<_> = appends
        .iter()
        .map(|append| (append.terminal_id.as_str(), append.data.as_str()))
        .collect();
    assert_eq!(
        appended,
        [
            ("terminal_1", "two"),
            ("terminal_2", "x"),
            ("terminal_1", "three")
        ]
    );
    assert!(reply.details.terminal_outputs.is_empty());
    let openaide_app_server_protocol::task::ActivityToolContent::Text { text } =
        &reply.details.content[0]
    else {
        panic!("expected the latest structured details");
    };
    assert_eq!(text, "second");
}

#[test]
fn a_tool_detail_version_ahead_of_the_artifact_replies_full() {
    let root = TempDir::new().unwrap();
    let store = open_store(&root);
    commit_artifact(&store, Some("first"), &[("terminal_1", "one")]);

    let reply = tool_detail(&store, Some(9));

    assert!(matches!(reply.terminal_sync, ToolTerminalSync::Full));
    assert_eq!(output_of(&reply, "terminal_1").as_deref(), Some("one"));
}

#[test]
fn a_tool_detail_subscription_still_checks_task_access() {
    let root = TempDir::new().unwrap();
    let store = open_store(&root);
    commit_artifact(&store, Some("first"), &[]);
    let mut other = super::tests::task_record("task-other");
    other.lifecycle = crate::storage::records::TaskLifecycle::Prepared {
        lease: Some(ClientInstanceId::from("client-2")),
    };
    store.write_task(&other).unwrap();

    let denied = TaskSnapshotStore::new(store).tool_detail_subscription(
        &client(),
        &TaskId::from("task-other"),
        ARTIFACT,
        None,
    );

    assert!(denied.is_err());
}

fn spawn_subagent(store: &Store) -> SubagentId {
    SubagentId::from(
        store
            .record_subagent_spawn(
                TASK,
                "root-session",
                "child",
                "Researcher".to_string(),
                "Inspect".to_string(),
                SubagentCapabilitiesRecord::default(),
                Vec::new(),
            )
            .unwrap()
            .subagent_id,
    )
}

fn subagent_says(store: &Store, id: &str, text: &str) {
    store
        .append_subagent_message(
            TASK,
            "child",
            NormalizedMessage::AgentMessage {
                id: id.to_string(),
                role: AgentMessageRole::Agent,
                parts: vec![AgentMessagePart::Text {
                    text: text.to_string(),
                }],
                created_at: "1".to_string(),
            },
            false,
        )
        .unwrap()
        .wait()
        .unwrap();
}

fn history(
    store: &Store,
    subagent: &SubagentId,
    known: Option<u64>,
) -> crate::snapshots::SubagentHistorySubscription {
    TaskSnapshotStore::new(store.clone())
        .subagent_history_subscription(&client(), &TaskId::from(TASK), subagent, known)
        .unwrap()
}

#[test]
fn a_subagent_history_without_a_known_version_replies_with_its_tail_page() {
    let root = TempDir::new().unwrap();
    let store = open_store(&root);
    let subagent = spawn_subagent(&store);
    subagent_says(&store, "m1", "hello");

    let reply = history(&store, &subagent, None);

    assert!(matches!(reply.chat_sync, SubagentChatSync::Full));
    assert_eq!(reply.history.revision, 1);
    assert_eq!(reply.history.chat.items.len(), 1);
    assert_eq!(
        reply.history.availability,
        SubagentHistoryAvailability::Available
    );
}

#[test]
fn the_current_subagent_history_version_replies_with_fields_and_no_chat() {
    let root = TempDir::new().unwrap();
    let store = open_store(&root);
    let subagent = spawn_subagent(&store);
    subagent_says(&store, "m1", "hello");

    let reply = history(&store, &subagent, Some(1));

    assert!(matches!(reply.chat_sync, SubagentChatSync::Current));
    assert_eq!(reply.history.revision, 1);
    assert_eq!(
        reply.history.availability,
        SubagentHistoryAvailability::Available
    );
    assert!(reply.history.chat.items.is_empty());
}

#[test]
fn a_stale_or_ahead_subagent_history_version_replies_full_without_a_change_log() {
    let root = TempDir::new().unwrap();
    let store = open_store(&root);
    let subagent = spawn_subagent(&store);
    subagent_says(&store, "m1", "hello");
    subagent_says(&store, "m2", "again");

    for known in [1, 7] {
        let reply = history(&store, &subagent, Some(known));
        assert!(
            matches!(reply.chat_sync, SubagentChatSync::Full),
            "known {known}"
        );
        assert_eq!(reply.history.revision, 2);
        assert_eq!(reply.history.chat.items.len(), 2);
    }
}

#[test]
fn a_subagent_history_version_survives_an_app_server_restart() {
    let root = TempDir::new().unwrap();
    let subagent = {
        let store = open_store(&root);
        let subagent = spawn_subagent(&store);
        subagent_says(&store, "m1", "hello");
        subagent_says(&store, "m2", "again");
        subagent
    };

    let reopened = Store::open(root.path().to_path_buf()).unwrap();
    let reply = history(&reopened, &subagent, Some(2));

    assert!(matches!(reply.chat_sync, SubagentChatSync::Current));
}

#[test]
fn an_unknown_subagent_is_not_found() {
    let root = TempDir::new().unwrap();
    let store = open_store(&root);

    let missing = TaskSnapshotStore::new(store).subagent_history_subscription(
        &client(),
        &TaskId::from(TASK),
        &SubagentId::from("subagent_missing"),
        None,
    );

    assert!(missing.is_err());
}
