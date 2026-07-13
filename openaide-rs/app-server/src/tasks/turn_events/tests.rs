use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use crate::agent::events::{
    AgentEvent, AgentPermissionOption, AgentPermissionOptionKind, AgentPermissionRequest,
    AgentToolCallRef,
};
use crate::agent::{
    AgentEventSink, AgentMetadataField, AgentSessionEventSink, AgentSessionMetadataUpdate,
    TurnCancellation,
};
use crate::client_lifecycle::AppServerTime;
use crate::protocol::model::{IsolationKind, NormalizedMessage, TaskStatus};
use crate::server_requests::ServerRequestRuntime;
use crate::server_requests::{ResponderScope, ServerRequestAnswer};
use crate::storage::records::{TaskPreparationRecord, TaskRecord, TaskTitle, TaskTitleSource};
use crate::storage::Store;
use crate::task_events::CommittedTaskDelta;
use crate::task_events::TaskUpdateNotifier;
use crate::tasks::mutation::TaskMutations;
use crate::tasks::runtime_state::RuntimeState;
use crate::tasks::transitions::TaskTransitions;
use openaide_app_server_protocol::ids::{ClientInstanceId, TaskId};
use openaide_app_server_protocol::server_requests::{
    QuestionField, QuestionRequestParams, QuestionRequestResponse, QuestionValue,
};

use super::{TaskEventSink, TaskSessionEventSink};

#[test]
fn agent_session_title_updates_set_and_clear_agent_owned_title() {
    let (_dir, store, mutations, _server_requests) = test_runtime();
    store.write_task(&running_task("task_1")).unwrap();
    let sink = TaskSessionEventSink::new(
        mutations,
        "task_1".to_string(),
        "session_1".to_string(),
        ServerRequestRuntime::new(),
    );

    sink.metadata_changed(AgentSessionMetadataUpdate {
        title: AgentMetadataField::Value("Agent title".to_string()),
        updated_at: AgentMetadataField::Unchanged,
    })
    .unwrap();
    let titled = store.read_task("task_1").unwrap();
    assert_eq!(
        titled.title,
        Some(TaskTitle::new("Agent title", TaskTitleSource::Agent).unwrap())
    );

    sink.metadata_changed(AgentSessionMetadataUpdate {
        title: AgentMetadataField::Clear,
        updated_at: AgentMetadataField::Unchanged,
    })
    .unwrap();
    let cleared = store.read_task("task_1").unwrap();
    assert_eq!(cleared.title, None);
}

#[test]
fn blank_agent_session_title_value_does_not_clear_the_agent_owned_title() {
    let (_dir, store, mutations, _server_requests) = test_runtime();
    let mut task = running_task("task_1");
    task.title = TaskTitle::new("Agent title", TaskTitleSource::Agent);
    store.write_task(&task).unwrap();
    let sink = TaskSessionEventSink::new(
        mutations,
        "task_1".to_string(),
        "session_1".to_string(),
        ServerRequestRuntime::new(),
    );

    sink.metadata_changed(AgentSessionMetadataUpdate {
        title: AgentMetadataField::Value("   ".to_string()),
        updated_at: AgentMetadataField::Unchanged,
    })
    .unwrap();

    assert_eq!(
        store.read_task("task_1").unwrap().title,
        TaskTitle::new("Agent title", TaskTitleSource::Agent)
    );
}

#[test]
fn agent_session_title_updates_never_replace_or_clear_a_user_owned_title() {
    let (_dir, store, mutations, _server_requests) = test_runtime();
    let mut task = running_task("task_1");
    task.title = TaskTitle::new("User title", TaskTitleSource::User);
    store.write_task(&task).unwrap();
    let sink = TaskSessionEventSink::new(
        mutations,
        "task_1".to_string(),
        "session_1".to_string(),
        ServerRequestRuntime::new(),
    );

    sink.metadata_changed(AgentSessionMetadataUpdate {
        title: AgentMetadataField::Value("Agent title".to_string()),
        updated_at: AgentMetadataField::Unchanged,
    })
    .unwrap();
    sink.metadata_changed(AgentSessionMetadataUpdate {
        title: AgentMetadataField::Clear,
        updated_at: AgentMetadataField::Unchanged,
    })
    .unwrap();

    assert_eq!(
        store.read_task("task_1").unwrap().title,
        TaskTitle::new("User title", TaskTitleSource::User)
    );
}

#[test]
fn agent_session_metadata_rejects_updates_from_a_stale_native_session() {
    let (_dir, store, mutations, _server_requests) = test_runtime();
    store.write_task(&running_task("task_1")).unwrap();
    let stale_sink = TaskSessionEventSink::new(
        mutations,
        "task_1".to_string(),
        "replaced-session".to_string(),
        ServerRequestRuntime::new(),
    );

    stale_sink
        .metadata_changed(AgentSessionMetadataUpdate {
            title: AgentMetadataField::Value("Stale title".to_string()),
            updated_at: AgentMetadataField::Value("2026-07-10T10:00:00Z".to_string()),
        })
        .unwrap();

    let task = store.read_task("task_1").unwrap();
    assert_eq!(task.title, None);
    assert_eq!(task.summary().title, None);
    assert_eq!(task.last_activity, "1");
}

#[test]
fn agent_session_catalogs_reject_updates_from_a_stale_native_session() {
    let (_dir, store, mutations, _server_requests) = test_runtime();
    store.write_task(&running_task("task_1")).unwrap();
    let stale_sink = TaskSessionEventSink::new(
        mutations,
        "task_1".to_string(),
        "replaced-session".to_string(),
        ServerRequestRuntime::new(),
    );

    stale_sink
        .config_options_changed(crate::protocol::model::ConfigOptionsCatalog::empty("codex"))
        .unwrap();
    stale_sink.commands_changed(Default::default()).unwrap();

    let task = store.read_task("task_1").unwrap();
    assert_eq!(task.config_options_catalog, None);
    assert_eq!(task.agent_commands_catalog, None);
    assert_eq!(task.revision, 0);
}

#[test]
fn repeated_identical_session_catalogs_do_not_churn_task_revision() {
    let (_dir, store, mutations, _server_requests) = test_runtime();
    store.write_task(&running_task("task_1")).unwrap();
    let sink = TaskSessionEventSink::new(
        mutations,
        "task_1".to_string(),
        "session_1".to_string(),
        ServerRequestRuntime::new(),
    );

    sink.config_options_changed(crate::protocol::model::ConfigOptionsCatalog::empty("codex"))
        .unwrap();
    sink.config_options_changed(crate::protocol::model::ConfigOptionsCatalog::empty("codex"))
        .unwrap();
    sink.commands_changed(Default::default()).unwrap();
    sink.commands_changed(Default::default()).unwrap();

    assert_eq!(store.read_task("task_1").unwrap().revision, 2);
}

#[test]
fn question_without_a_capable_responder_is_cancelled_without_blocking_the_task() {
    let (_dir, store, mutations, server_requests) = test_runtime();
    store.write_task(&running_task("task_1")).unwrap();
    let sink = TaskSessionEventSink::new(
        mutations,
        "task_1".to_string(),
        "session_1".to_string(),
        server_requests.clone(),
    );

    let response = sink
        .request_question(question_form(), TurnCancellation::new())
        .unwrap();

    assert_eq!(response, QuestionRequestResponse::Cancel);
    assert_eq!(
        store.read_task("task_1").unwrap().status,
        TaskStatus::Active
    );
    assert!(server_requests
        .pending_for_task(&TaskId::from("task_1"))
        .is_empty());
}

#[test]
fn session_question_round_trips_and_persists_submitted_history() {
    let (_dir, store, mutations, server_requests) = test_runtime();
    register_question_responder(&server_requests, "task_1");
    store.write_task(&running_task("task_1")).unwrap();
    let sink = TaskSessionEventSink::new(
        mutations,
        "task_1".to_string(),
        "session_1".to_string(),
        server_requests.clone(),
    );
    let thread =
        std::thread::spawn(move || sink.request_question(question_form(), TurnCancellation::new()));
    while server_requests
        .pending_for_task(&TaskId::from("task_1"))
        .is_empty()
    {
        std::thread::yield_now();
    }
    let request = server_requests.pending_for_task(&TaskId::from("task_1"))[0].clone();
    let result = serde_json::to_value(QuestionRequestResponse::Submit {
        content: BTreeMap::from([(
            "strategy".to_string(),
            QuestionValue::String("safe".to_string()),
        )]),
    })
    .unwrap();
    assert!(matches!(
        server_requests.handle_response_from_scopes(
            ClientInstanceId::from("client-1"),
            request.request_id,
            ServerRequestAnswer::Result(result),
            &[ResponderScope::Task(TaskId::from("task_1"))],
            AppServerTime(1),
        ),
        crate::server_requests::ResponseOutcome::Accepted { .. }
    ));
    assert!(matches!(
        thread.join().unwrap().unwrap(),
        QuestionRequestResponse::Submit { .. }
    ));
    let messages = store.read_messages("task_1").unwrap();
    assert!(messages.iter().any(|stored| matches!(&stored.chat.message,
        NormalizedMessage::Question { state: crate::protocol::model::QuestionState::Resolved,
            content: Some(content), .. }
            if content.get("strategy") == Some(&QuestionValue::String("safe".to_string())))));
}

#[test]
fn withdrawing_one_question_closes_only_its_own_waiter() {
    let (_dir, store, mutations, server_requests) = test_runtime();
    register_question_responder(&server_requests, "task_1");
    store.write_task(&running_task("task_1")).unwrap();
    let cancellation = TurnCancellation::new();
    let sink = TaskSessionEventSink::new(
        mutations,
        "task_1".to_string(),
        "session_1".to_string(),
        server_requests.clone(),
    );
    let wait_cancellation = cancellation.clone();
    let thread =
        std::thread::spawn(move || sink.request_question(question_form(), wait_cancellation));
    while server_requests.pending_count() == 0 {
        std::thread::yield_now();
    }
    cancellation.cancel();

    assert_eq!(
        thread.join().unwrap().unwrap(),
        QuestionRequestResponse::Cancel
    );
    let messages = store.read_messages("task_1").unwrap();
    assert!(messages.iter().any(|stored| matches!(
        stored.chat.message,
        NormalizedMessage::Question {
            state: crate::protocol::model::QuestionState::Cancelled,
            ..
        }
    )));
}

fn question_form() -> QuestionRequestParams {
    QuestionRequestParams {
        message: "Choose a strategy".to_string(),
        fields: vec![QuestionField::SingleSelect {
            key: "strategy".to_string(),
            title: "Strategy".to_string(),
            description: None,
            required: true,
            default: Some("safe".to_string()),
            options: vec![
                openaide_app_server_protocol::server_requests::QuestionOption {
                    value: "safe".to_string(),
                    label: "Safe".to_string(),
                    description: Some("Small changes".to_string()),
                },
            ],
        }],
    }
}

fn register_question_responder(server_requests: &ServerRequestRuntime, task_id: &str) {
    server_requests.observe_subscription_added(
        crate::client_lifecycle::Delivery::new(
            ClientInstanceId::from("client-1"),
            crate::client_lifecycle::ConnectionId::new("conn-1"),
        )
        .with_request_capabilities(vec![crate::client_lifecycle::RequestCapability::Question]),
        TaskId::from(task_id),
        AppServerTime(0),
    );
}

fn register_permission_responder(server_requests: &ServerRequestRuntime, task_id: &str) {
    server_requests.observe_subscription_added(
        crate::client_lifecycle::Delivery::new(
            ClientInstanceId::from("client-1"),
            crate::client_lifecycle::ConnectionId::new("conn-1"),
        )
        .with_request_capabilities(vec![crate::client_lifecycle::RequestCapability::Permission]),
        TaskId::from(task_id),
        AppServerTime(0),
    );
}

#[test]
fn permission_without_a_capable_responder_is_cancelled_without_blocking_the_task() {
    let (_dir, store, mutations, server_requests) = test_runtime();
    store.write_task(&running_task("task_1")).unwrap();
    let sink = TaskEventSink::new(
        mutations,
        "task_1".to_string(),
        "turn_1".to_string(),
        server_requests.clone(),
        TurnCancellation::new(),
    );

    let outcome = sink
        .request_permission(permission_request("permission_1"))
        .unwrap();

    assert!(matches!(
        outcome,
        crate::agent::events::AgentPermissionOutcome::Cancelled
    ));
    assert_eq!(
        store.read_task("task_1").unwrap().status,
        TaskStatus::Active
    );
    assert_eq!(server_requests.pending_count(), 0);
}

#[test]
fn permission_request_append_failure_does_not_open_broker_request() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().to_path_buf()).unwrap();
    let (notifier, _notifications) = TaskUpdateNotifier::channel();
    let mutations = TaskMutations::new(
        store,
        Arc::new(Mutex::new(())),
        Arc::new(Mutex::new(RuntimeState::with_revision(0))),
        notifier,
    );
    let server_requests = ServerRequestRuntime::new();
    register_permission_responder(&server_requests, "missing_task");
    let sink = TaskEventSink::new(
        mutations,
        "missing_task".to_string(),
        "turn_1".to_string(),
        server_requests.clone(),
        TurnCancellation::new(),
    );

    let error = sink.request_permission(permission_request("request_append_failure"));

    assert!(error.is_err());
    assert!(server_requests
        .pending_for_task(&openaide_app_server_protocol::ids::TaskId::from(
            "missing_task"
        ))
        .is_empty());
}

#[test]
fn active_turn_agent_message_does_not_mark_task_unread() {
    let (_dir, store, mutations, server_requests) = test_runtime();
    store.write_task(&running_task("task_1")).unwrap();
    let sink = TaskEventSink::new(
        mutations,
        "task_1".to_string(),
        "turn_1".to_string(),
        server_requests,
        TurnCancellation::new(),
    );

    sink.emit(AgentEvent::Text("working".to_string())).unwrap();

    let stored = store.read_task("task_1").unwrap();
    assert!(!stored.unread);
    assert_eq!(stored.active_turn_id.as_deref(), Some("turn_1"));
    assert_eq!(stored.status, TaskStatus::Active);
}

#[test]
fn active_turn_agent_message_does_not_refresh_last_activity() {
    let (_dir, store, mutations, server_requests) = test_runtime();
    store.write_task(&running_task("task_1")).unwrap();
    let sink = TaskEventSink::new(
        mutations,
        "task_1".to_string(),
        "turn_1".to_string(),
        server_requests,
        TurnCancellation::new(),
    );

    sink.emit(AgentEvent::Text("working".to_string())).unwrap();

    let stored = store.read_task("task_1").unwrap();
    assert_ne!(stored.updated_at, "1");
    assert_eq!(stored.last_activity, "1");
}

#[test]
fn native_session_update_is_persisted_after_prompt_completion() {
    let (_dir, store, mutations, server_requests) = test_runtime();
    store.write_task(&running_task("task_1")).unwrap();
    let sink = TaskSessionEventSink::new(
        mutations.clone(),
        "task_1".to_string(),
        "session_1".to_string(),
        server_requests,
    );

    sink.session_update(AgentEvent::TextChunk {
        text: "before".to_string(),
        source_message_id: Some("agent-message-1".to_string()),
    })
    .unwrap();
    TaskTransitions::new(mutations)
        .finish_turn("task_1", "turn_1", Ok(()))
        .unwrap();

    sink.session_update(AgentEvent::TextChunk {
        text: " after".to_string(),
        source_message_id: Some("agent-message-1".to_string()),
    })
    .unwrap();

    let messages = store.read_messages("task_1").unwrap();
    let message = messages
        .iter()
        .find(|stored| matches!(stored.chat.message, NormalizedMessage::AgentText { .. }))
        .expect("Agent message is retained after prompt completion");
    assert_eq!(
        message.chat.identity,
        "acp:session_1:message:agent-message-1"
    );
    assert_eq!(message.chat.message_id, message.chat.identity);
    assert!(matches!(
        &message.chat.message,
        NormalizedMessage::AgentText { id, text, .. }
            if id == &message.chat.identity && text == "before after"
    ));
}

#[test]
fn agent_text_notifications_describe_only_durable_ordered_deltas() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().to_path_buf()).unwrap();
    store.write_task(&running_task("task_1")).unwrap();
    let (notifier, notifications) = TaskUpdateNotifier::channel();
    let mutations = TaskMutations::new(
        store.clone(),
        Arc::new(Mutex::new(())),
        Arc::new(Mutex::new(RuntimeState::with_revision(0))),
        notifier,
    );
    let sink = TaskEventSink::new(
        mutations,
        "task_1".to_string(),
        "turn_1".to_string(),
        ServerRequestRuntime::new(),
        TurnCancellation::new(),
    );

    sink.emit(AgentEvent::Text("first".to_string())).unwrap();
    let appended = notifications.recv().unwrap();
    let message_id = match appended.delta.unwrap() {
        CommittedTaskDelta::ChatItemAppended { item } => {
            assert_eq!(
                item.status,
                openaide_app_server_protocol::snapshot::ChatItemStatus::Complete
            );
            item.message_id
        }
        other => panic!("expected append, got {other:?}"),
    };
    assert_eq!(store.read_messages("task_1").unwrap().len(), 1);

    sink.emit(AgentEvent::Text(" second".to_string())).unwrap();
    let chunked = notifications.recv().unwrap();
    assert!(matches!(
        chunked.delta,
        Some(CommittedTaskDelta::ChatItemChunk { message_id: id, chunk })
            if id == message_id && chunk.text == " second"
    ));
    let stored = store.read_messages("task_1").unwrap();
    assert!(matches!(
        &stored[0].chat.message,
        NormalizedMessage::AgentText { text, .. }
            if text == "first second"
    ));

    sink.emit(AgentEvent::Activity {
        title: "Tool".to_string(),
        tool_name: "fixture".to_string(),
        output_preview: "done".to_string(),
    })
    .unwrap();
    let activity_update = notifications.recv().unwrap();
    assert!(activity_update.delta.is_none());
}

#[test]
fn interleaved_source_message_ids_update_their_original_agent_messages() {
    let (_dir, store, mutations, server_requests) = test_runtime();
    store.write_task(&running_task("task_1")).unwrap();
    let sink = TaskEventSink::new(
        mutations,
        "task_1".to_string(),
        "turn_1".to_string(),
        server_requests,
        TurnCancellation::new(),
    );

    for (source_message_id, text) in [
        ("message-a", "A1"),
        ("message-b", "B1"),
        ("message-a", "A2"),
        ("message-b", "B2"),
    ] {
        sink.emit(AgentEvent::TextChunk {
            text: text.to_string(),
            source_message_id: Some(source_message_id.to_string()),
        })
        .unwrap();
    }

    let messages = store.read_messages("task_1").unwrap();
    let agent_texts = messages
        .iter()
        .filter_map(|stored| match &stored.chat.message {
            NormalizedMessage::AgentText { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(agent_texts, ["A1A2", "B1B2"]);
}

#[test]
fn prompt_completion_does_not_change_session_owned_text() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().to_path_buf()).unwrap();
    store.write_task(&running_task("task_1")).unwrap();
    let (notifier, notifications) = TaskUpdateNotifier::channel();
    let mutations = TaskMutations::new(
        store.clone(),
        Arc::new(Mutex::new(())),
        Arc::new(Mutex::new(RuntimeState::with_revision(0))),
        notifier,
    );
    let sink = TaskEventSink::new(
        mutations,
        "task_1".to_string(),
        "turn_1".to_string(),
        ServerRequestRuntime::new(),
        TurnCancellation::new(),
    );

    sink.emit(AgentEvent::Text("complete me".to_string()))
        .unwrap();
    let _appended = notifications.recv().unwrap();
    assert!(matches!(
        &store.read_messages("task_1").unwrap()[0].chat.message,
        NormalizedMessage::AgentText { .. }
    ));
}

#[test]
fn permission_request_splits_active_agent_text_run() {
    let (_dir, store, mutations, server_requests) = test_runtime();
    store.write_task(&running_task("task_1")).unwrap();
    register_permission_responder(&server_requests, "task_1");
    let sink = Arc::new(TaskEventSink::new(
        mutations,
        "task_1".to_string(),
        "turn_1".to_string(),
        server_requests.clone(),
        TurnCancellation::new(),
    ));

    sink.emit(AgentEvent::Text("before permission".to_string()))
        .unwrap();
    let permission_sink = sink.clone();
    let permission_thread = std::thread::spawn(move || {
        permission_sink.request_permission(permission_request("permission_1"))
    });
    while server_requests.pending_count() == 0 {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    server_requests
        .route_agent_permission_response(
            "permission_1",
            "allow".to_string(),
            |_| -> Result<(), crate::protocol::errors::RuntimeError> { Ok(()) },
        )
        .unwrap();
    permission_thread
        .join()
        .expect("permission thread joins")
        .unwrap();

    sink.emit(AgentEvent::Text(" after permission".to_string()))
        .unwrap();

    let messages = store.read_messages("task_1").unwrap();
    let agent_text: Vec<_> = messages
        .iter()
        .filter_map(|stored| match &stored.chat.message {
            NormalizedMessage::AgentText { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(agent_text, vec!["before permission", " after permission"]);
}

#[test]
fn permission_wait_does_not_block_concurrent_agent_events() {
    let (_dir, store, mutations, server_requests) = test_runtime();
    store.write_task(&running_task("task_1")).unwrap();
    register_permission_responder(&server_requests, "task_1");
    let sink = Arc::new(TaskEventSink::new(
        mutations,
        "task_1".to_string(),
        "turn_1".to_string(),
        server_requests.clone(),
        TurnCancellation::new(),
    ));

    let permission_sink = sink.clone();
    let permission_thread = std::thread::spawn(move || {
        permission_sink.request_permission(permission_request("permission_1"))
    });
    while server_requests.pending_count() == 0 {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    let (emit_done_tx, emit_done_rx) = std::sync::mpsc::channel();
    let event_sink = sink.clone();
    let event_thread = std::thread::spawn(move || {
        let result = event_sink.emit(AgentEvent::Text("while waiting".to_string()));
        let _ = emit_done_tx.send(result);
    });
    let emitted_while_waiting = emit_done_rx
        .recv_timeout(std::time::Duration::from_millis(250))
        .is_ok();

    server_requests
        .route_agent_permission_response(
            "permission_1",
            "allow".to_string(),
            |_| -> Result<(), crate::protocol::errors::RuntimeError> { Ok(()) },
        )
        .unwrap();
    permission_thread
        .join()
        .expect("permission thread joins")
        .unwrap();
    event_thread.join().expect("event thread joins");

    assert!(
        emitted_while_waiting,
        "agent events must continue while a permission decision is pending"
    );
    assert!(store.read_messages("task_1").unwrap().iter().any(|stored| {
        matches!(
            &stored.chat.message,
            NormalizedMessage::AgentText { text, .. } if text == "while waiting"
        )
    }));
}

#[test]
fn finishing_active_turn_marks_task_unread_for_user_attention() {
    let (_dir, store, mutations, _server_requests) = test_runtime();
    store.write_task(&running_task("task_1")).unwrap();
    let transitions = TaskTransitions::new(mutations);

    assert!(transitions.finish_turn("task_1", "turn_1", Ok(())).unwrap());

    let stored = store.read_task("task_1").unwrap();
    assert!(stored.unread);
    assert_eq!(stored.active_turn_id, None);
    assert_eq!(stored.status, TaskStatus::Inactive);
}

fn permission_request(request_id: &str) -> AgentPermissionRequest {
    AgentPermissionRequest {
        request_id: request_id.to_string(),
        title: "Allow action?".to_string(),
        description: None,
        scope: None,
        risk: None,
        tool_call: AgentToolCallRef {
            tool_call_id: "tool_1".to_string(),
            title: "Tool".to_string(),
            kind: Some("edit".to_string()),
        },
        options: vec![AgentPermissionOption {
            option_id: "allow".to_string(),
            name: "Allow".to_string(),
            kind: AgentPermissionOptionKind::AllowOnce,
        }],
    }
}

fn test_runtime() -> (
    tempfile::TempDir,
    Store,
    TaskMutations,
    ServerRequestRuntime,
) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().to_path_buf()).unwrap();
    let (notifier, _notifications) = TaskUpdateNotifier::channel();
    let mutations = TaskMutations::new(
        store.clone(),
        Arc::new(Mutex::new(())),
        Arc::new(Mutex::new(RuntimeState::with_revision(0))),
        notifier,
    );
    (dir, store, mutations, ServerRequestRuntime::new())
}

fn running_task(task_id: &str) -> TaskRecord {
    TaskRecord {
        task_id: task_id.to_string(),
        title: None,
        status: TaskStatus::Active,
        task_version: 0,
        message_history_version: 0,
        unread: false,
        created_at: "1".to_string(),
        updated_at: "1".to_string(),
        last_activity: "1".to_string(),
        agent_name: "Codex".to_string(),
        agent_id: "codex".to_string(),
        isolation: IsolationKind::Local,
        workspace_root: "/tmp/workspace".to_string(),
        lifecycle: crate::storage::records::TaskLifecycle::Visible,
        agent_session_id: Some("session_1".to_string()),
        active_turn_id: Some("turn_1".to_string()),
        archived: false,
        tombstoned: false,
        revision: 0,
        config_options: Default::default(),
        config_options_catalog: None,
        config_mutation: Default::default(),
        agent_commands_catalog: None,
        model_id: None,
        preparation: TaskPreparationRecord::Ready,
    }
}
