use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use crate::agent::events::{
    AgentEvent, AgentPermissionOption, AgentPermissionOptionKind, AgentPermissionRequest,
    AgentTerminalAppend, AgentToolCall, AgentToolCallRef, AgentToolCallStatus, AgentToolUpdate,
};
use crate::agent::{
    AgentEventSink, AgentMetadataField, AgentPromptOutcome, AgentSessionEventSink,
    AgentSessionMetadataUpdate, TurnCancellation,
};
use crate::client_lifecycle::AppServerTime;
use crate::native_sessions::catalog::{
    NativeSessionCatalog, NativeSessionObservation, NativeSessionRef,
};
use crate::protocol::model::{
    ActivityStatus, ActivityToolDetails, ActivityToolOutput, AgentMessagePart, AgentMessageRole,
    IsolationKind, NormalizedMessage, TaskStatus,
};
use crate::server_requests::ServerRequestRuntime;
use crate::server_requests::{ResponderScope, ServerRequestAnswer};
use crate::storage::records::{
    TaskAttentionReason, TaskPreparationRecord, TaskRecord, TaskTitle, TaskTitleSource,
};
use crate::storage::Store;
use crate::task_events::TaskUpdateKind;
use crate::task_events::TaskUpdateNotifier;
use crate::tasks::mutation::TaskMutations;
use crate::tasks::runtime_state::RuntimeState;
use crate::tasks::transitions::TaskTransitions;
use openaide_app_server_protocol::events::TaskChatChange;
use openaide_app_server_protocol::ids::{ClientInstanceId, TaskId};
use openaide_app_server_protocol::server_requests::{
    QuestionField, QuestionRequestParams, QuestionRequestResponse, QuestionValue,
};

use super::{TaskEventSink, TaskSessionEventSink};

fn agent_text_event(text: &str) -> AgentEvent {
    AgentEvent::MessageChunk {
        role: AgentMessageRole::Agent,
        part: AgentMessagePart::Text {
            text: text.to_string(),
        },
        source_message_id: None,
    }
}

fn sourced_agent_text_event(text: &str, source_message_id: &str) -> AgentEvent {
    AgentEvent::MessageChunk {
        role: AgentMessageRole::Agent,
        part: AgentMessagePart::Text {
            text: text.to_string(),
        },
        source_message_id: Some(source_message_id.to_string()),
    }
}

fn agent_message_text(message: &NormalizedMessage) -> Option<&str> {
    match message {
        NormalizedMessage::AgentMessage {
            role: AgentMessageRole::Agent,
            parts,
            ..
        } => match parts.as_slice() {
            [AgentMessagePart::Text { text }] => Some(text),
            _ => None,
        },
        _ => None,
    }
}

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
fn live_session_metadata_updates_the_durable_native_catalog() {
    let (_dir, store, mutations, _server_requests) = test_runtime();
    store.write_task(&running_task("task_1")).unwrap();
    let catalog = NativeSessionCatalog::open(store).unwrap();
    let reference = NativeSessionRef::new("codex", "session_1");
    catalog
        .record_page(
            "project-1",
            "/workspace",
            vec![NativeSessionObservation {
                reference: reference.clone(),
                title: Some("Listed title".to_string()),
                last_activity: Some("2026-07-21T12:00:00Z".to_string()),
            }],
        )
        .unwrap();
    let sink = TaskSessionEventSink::new(
        mutations,
        "task_1".to_string(),
        "session_1".to_string(),
        ServerRequestRuntime::new(),
    )
    .with_native_catalog(Some(catalog.clone()));

    sink.metadata_changed(AgentSessionMetadataUpdate {
        title: AgentMetadataField::Value("Live title".to_string()),
        updated_at: AgentMetadataField::Value("2026-07-21T13:00:00Z".to_string()),
    })
    .unwrap();

    assert_eq!(
        catalog.entry(&reference).unwrap().observation,
        NativeSessionObservation {
            reference,
            title: Some("Live title".to_string()),
            last_activity: Some("2026-07-21T13:00:00Z".to_string()),
        }
    );
}

#[test]
fn agent_session_title_clear_overrides_prompt_title() {
    let (_dir, store, mutations, _server_requests) = test_runtime();
    let mut task = running_task("task_1");
    task.title = TaskTitle::new("Prompt fallback", TaskTitleSource::Prompt);
    store.write_task(&task).unwrap();
    let sink = TaskSessionEventSink::new(
        mutations,
        "task_1".to_string(),
        "session_1".to_string(),
        ServerRequestRuntime::new(),
    );

    sink.metadata_changed(AgentSessionMetadataUpdate {
        title: AgentMetadataField::Clear,
        updated_at: AgentMetadataField::Unchanged,
    })
    .unwrap();

    assert_eq!(store.read_task("task_1").unwrap().title, None);
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
fn agent_session_metadata_never_moves_task_activity_backwards() {
    let (_dir, store, mutations, _server_requests) = test_runtime();
    let mut task = running_task("task_1");
    task.last_activity = "2026-07-10T10:00:00Z".to_string();
    store.write_task(&task).unwrap();
    let sink = TaskSessionEventSink::new(
        mutations,
        "task_1".to_string(),
        "session_1".to_string(),
        ServerRequestRuntime::new(),
    );

    sink.metadata_changed(AgentSessionMetadataUpdate {
        title: AgentMetadataField::Unchanged,
        updated_at: AgentMetadataField::Value("2026-07-10T09:00:00Z".to_string()),
    })
    .unwrap();

    assert_eq!(
        store.read_task("task_1").unwrap().last_activity,
        "2026-07-10T10:00:00Z",
    );
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
fn question_waits_for_a_late_responder() {
    let (_dir, store, mutations, server_requests) = test_runtime();
    store.write_task(&running_task("task_1")).unwrap();
    let sink = TaskSessionEventSink::new(
        mutations,
        "task_1".to_string(),
        "session_1".to_string(),
        server_requests.clone(),
    );

    let thread =
        std::thread::spawn(move || sink.request_question(question_form(), TurnCancellation::new()));
    while server_requests.pending_count() == 0 {
        std::thread::yield_now();
    }
    while store.read_task("task_1").unwrap().status != TaskStatus::Waiting {
        std::thread::yield_now();
    }
    assert_eq!(
        store.read_task("task_1").unwrap().status,
        TaskStatus::Waiting
    );
    let waiting = store.read_task("task_1").unwrap();
    assert_eq!(
        waiting.attention.as_ref().map(|event| event.reason),
        Some(TaskAttentionReason::NeedsAnswer)
    );
    register_question_responder(&server_requests, "task_1");
    let request = server_requests.pending_for_task(&TaskId::from("task_1"))[0].clone();
    assert!(waiting.attention.as_ref().is_some_and(|event| event
        .event_id
        .starts_with(&format!("request:{}:", request.request_id.as_str()))));
    assert!(matches!(
        server_requests.handle_response_from_scopes(
            ClientInstanceId::from("client-1"),
            request.request_id,
            ServerRequestAnswer::Result(
                serde_json::to_value(QuestionRequestResponse::Cancel).unwrap(),
            ),
            &[ResponderScope::Task(TaskId::from("task_1"))],
            AppServerTime(1),
        ),
        crate::server_requests::ResponseOutcome::Accepted { .. }
    ));
    assert_eq!(
        thread.join().unwrap().unwrap(),
        QuestionRequestResponse::Cancel
    );
}

#[test]
fn resolving_one_of_two_questions_keeps_task_waiting_for_the_other() {
    let (_dir, store, mutations, server_requests) = test_runtime();
    register_question_responder(&server_requests, "task_1");
    store.write_task(&running_task("task_1")).unwrap();
    let sink = Arc::new(TaskSessionEventSink::new(
        mutations,
        "task_1".to_string(),
        "session_1".to_string(),
        server_requests.clone(),
    ));

    let first_sink = Arc::clone(&sink);
    let first = std::thread::spawn(move || {
        first_sink.request_question(question_form(), TurnCancellation::new())
    });
    while server_requests.pending_count() != 1 {
        std::thread::yield_now();
    }
    let second_sink = Arc::clone(&sink);
    let second = std::thread::spawn(move || {
        second_sink.request_question(question_form(), TurnCancellation::new())
    });
    while server_requests.pending_count() != 2 {
        std::thread::yield_now();
    }

    let task_id = TaskId::from("task_1");
    let requests = server_requests.pending_for_task(&task_id);
    answer_question(&server_requests, &task_id, requests[0].request_id.clone());
    assert_eq!(
        first.join().unwrap().unwrap(),
        QuestionRequestResponse::Cancel
    );
    assert_eq!(
        store.read_task("task_1").unwrap().status,
        TaskStatus::Waiting
    );

    let remaining = server_requests.pending_for_task(&task_id);
    assert_eq!(remaining.len(), 1);
    assert!(store
        .read_task("task_1")
        .unwrap()
        .attention
        .as_ref()
        .is_some_and(|event| event
            .event_id
            .starts_with(&format!("request:{}:", remaining[0].request_id.as_str()))));
    answer_question(&server_requests, &task_id, remaining[0].request_id.clone());
    assert_eq!(
        second.join().unwrap().unwrap(),
        QuestionRequestResponse::Cancel
    );
    assert_eq!(
        store.read_task("task_1").unwrap().status,
        TaskStatus::Active
    );
    assert!(store.read_task("task_1").unwrap().attention.is_none());
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
    while store.read_task("task_1").unwrap().status != TaskStatus::Waiting {
        std::thread::yield_now();
    }
    assert!(store.read_messages("task_1").unwrap().is_empty());
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
    while store.read_task("task_1").unwrap().status != TaskStatus::Waiting {
        std::thread::yield_now();
    }
    assert!(store.read_messages("task_1").unwrap().is_empty());
    cancellation.cancel();

    assert_eq!(
        thread.join().unwrap().unwrap(),
        QuestionRequestResponse::Cancel
    );
    let messages = store.read_messages("task_1").unwrap();
    assert!(messages.iter().any(|stored| matches!(
        &stored.chat.message,
        NormalizedMessage::Question {
            state: crate::protocol::model::QuestionState::Cancelled,
            resolution_message: Some(message),
            ..
        } if message == "Task stopped while a question was pending."
    )));
}

#[test]
fn permission_is_transient_until_the_server_request_resolves() {
    let (_dir, store, mutations, server_requests) = test_runtime();
    register_permission_responder(&server_requests, "task_1");
    store.write_task(&running_task("task_1")).unwrap();
    let sink = TaskEventSink::new(
        mutations,
        "task_1".to_string(),
        "turn_1".to_string(),
        server_requests.clone(),
        TurnCancellation::new(),
    );
    sink.emit(AgentEvent::ToolCall(AgentToolCall {
        tool_call_id: "tool_1".to_string(),
        scope_id: None,
        title: "Editing".to_string(),
        kind: "edit".to_string(),
        status: AgentToolCallStatus::Pending,
        input_summary: None,
        output_preview: None,
        details: None,
    }))
    .unwrap();

    let thread =
        std::thread::spawn(move || sink.request_permission(permission_request("permission_1")));
    while server_requests.pending_count() == 0 {
        std::thread::yield_now();
    }
    while store.read_task("task_1").unwrap().status != TaskStatus::Waiting {
        std::thread::yield_now();
    }
    assert_eq!(store.read_messages("task_1").unwrap().len(), 1);

    let request = server_requests.pending_for_task(&TaskId::from("task_1"))[0].clone();
    assert!(matches!(
        server_requests.handle_response_from_scopes(
            ClientInstanceId::from("client-1"),
            request.request_id,
            ServerRequestAnswer::Result(serde_json::json!({ "optionId": "allow" })),
            &[ResponderScope::Task(TaskId::from("task_1"))],
            AppServerTime(1),
        ),
        crate::server_requests::ResponseOutcome::Accepted { .. }
    ));
    assert!(matches!(
        thread.join().unwrap().unwrap(),
        crate::agent::events::AgentPermissionOutcome::Selected { option_id } if option_id == "allow"
    ));

    let messages = store.read_messages("task_1").unwrap();
    assert_eq!(messages.len(), 1);
    assert!(matches!(
        &messages[0].chat.message,
        NormalizedMessage::Activity { steps, .. }
            if matches!(
                steps.as_slice(),
                [crate::protocol::model::ActivityStep::Tool { permission_outcomes, .. }]
                    if matches!(permission_outcomes.as_slice(), [outcome]
                        if outcome.request_id == "server-request-1"
                            && outcome.decision == crate::protocol::model::ToolPermissionDecision::Approved
                            && outcome.option_id.as_deref() == Some("allow")
                            && outcome.option_label.as_deref() == Some("Allow"))
            )
    ));
}

#[test]
fn multiple_permission_decisions_remain_linked_after_later_tool_updates() {
    let (_dir, store, mutations, server_requests) = test_runtime();
    register_permission_responder(&server_requests, "task_1");
    store.write_task(&running_task("task_1")).unwrap();
    let sink = Arc::new(TaskEventSink::new(
        mutations,
        "task_1".to_string(),
        "turn_1".to_string(),
        server_requests.clone(),
        TurnCancellation::new(),
    ));
    sink.emit(tool_event(AgentToolCallStatus::Pending)).unwrap();

    for (request_id, option_id) in [("permission_1", "allow"), ("permission_2", "reject")] {
        let request_sink = Arc::clone(&sink);
        let request_id = request_id.to_string();
        let thread = std::thread::spawn(move || {
            request_sink.request_permission(permission_request(&request_id))
        });
        while server_requests.pending_count() == 0 {
            std::thread::yield_now();
        }
        answer_permission(&server_requests, "task_1", option_id);
        thread.join().unwrap().unwrap();
    }
    sink.emit(tool_event(AgentToolCallStatus::Completed))
        .unwrap();

    let messages = store.read_messages("task_1").unwrap();
    assert_eq!(messages.len(), 1);
    assert!(matches!(
        &messages[0].chat.message,
        NormalizedMessage::Activity {
            status: ActivityStatus::Completed,
            steps,
            ..
        } if matches!(
            steps.as_slice(),
            [crate::protocol::model::ActivityStep::Tool { permission_outcomes, .. }]
                if permission_outcomes.len() == 2
                    && permission_outcomes[0].decision
                        == crate::protocol::model::ToolPermissionDecision::Approved
                    && permission_outcomes[1].decision
                        == crate::protocol::model::ToolPermissionDecision::Rejected
        )
    ));
}

#[test]
fn cancelling_a_waiting_permission_records_cancelled_outcome_on_the_tool() {
    let (_dir, store, mutations, server_requests) = test_runtime();
    register_permission_responder(&server_requests, "task_1");
    store.write_task(&running_task("task_1")).unwrap();
    let cancellation = TurnCancellation::new();
    let sink = TaskEventSink::new(
        mutations,
        "task_1".to_string(),
        "turn_1".to_string(),
        server_requests.clone(),
        cancellation.clone(),
    );
    sink.emit(tool_event(AgentToolCallStatus::Pending)).unwrap();

    let thread =
        std::thread::spawn(move || sink.request_permission(permission_request("permission_1")));
    while server_requests.pending_count() == 0 {
        std::thread::yield_now();
    }
    while store.read_task("task_1").unwrap().status != TaskStatus::Waiting {
        std::thread::yield_now();
    }
    assert_eq!(store.read_messages("task_1").unwrap().len(), 1);
    cancellation.cancel();

    assert!(matches!(
        thread.join().unwrap().unwrap(),
        crate::agent::events::AgentPermissionOutcome::Cancelled
    ));
    let messages = store.read_messages("task_1").unwrap();
    assert_eq!(messages.len(), 1);
    assert!(matches!(
        &messages[0].chat.message,
        NormalizedMessage::Activity { steps, .. }
            if matches!(
                steps.as_slice(),
                [crate::protocol::model::ActivityStep::Tool { permission_outcomes, .. }]
                    if matches!(permission_outcomes.as_slice(), [outcome]
                        if outcome.request_id == "server-request-1"
                            && outcome.decision
                                == crate::protocol::model::ToolPermissionDecision::Cancelled)
            )
    ));
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

fn answer_permission(server_requests: &ServerRequestRuntime, task_id: &str, option_id: &str) {
    let task_id = TaskId::from(task_id);
    let request = server_requests.pending_for_task(&task_id)[0].clone();
    assert!(matches!(
        server_requests.handle_response_from_scopes(
            ClientInstanceId::from("client-1"),
            request.request_id,
            ServerRequestAnswer::Result(serde_json::json!({ "optionId": option_id })),
            &[ResponderScope::Task(task_id)],
            AppServerTime(1),
        ),
        crate::server_requests::ResponseOutcome::Accepted { .. }
    ));
}

fn answer_question(
    server_requests: &ServerRequestRuntime,
    task_id: &TaskId,
    request_id: openaide_app_server_protocol::ids::RequestId,
) {
    assert!(matches!(
        server_requests.handle_response_from_scopes(
            ClientInstanceId::from("client-1"),
            request_id,
            ServerRequestAnswer::Result(
                serde_json::to_value(QuestionRequestResponse::Cancel).unwrap(),
            ),
            &[ResponderScope::Task(task_id.clone())],
            AppServerTime(1),
        ),
        crate::server_requests::ResponseOutcome::Accepted { .. }
    ));
}

#[test]
fn permission_waits_for_a_late_responder() {
    let (_dir, store, mutations, server_requests) = test_runtime();
    store.write_task(&running_task("task_1")).unwrap();
    let sink = TaskEventSink::new(
        mutations,
        "task_1".to_string(),
        "turn_1".to_string(),
        server_requests.clone(),
        TurnCancellation::new(),
    );
    sink.emit(tool_event(AgentToolCallStatus::Pending)).unwrap();

    let thread =
        std::thread::spawn(move || sink.request_permission(permission_request("permission_1")));
    while server_requests.pending_count() == 0 {
        std::thread::yield_now();
    }
    while store.read_task("task_1").unwrap().status != TaskStatus::Waiting {
        std::thread::yield_now();
    }
    assert_eq!(
        store.read_task("task_1").unwrap().status,
        TaskStatus::Waiting
    );
    let request = server_requests.pending_for_task(&TaskId::from("task_1"))[0].clone();
    let waiting = store.read_task("task_1").unwrap();
    assert_eq!(
        waiting.attention.as_ref().map(|event| event.reason),
        Some(TaskAttentionReason::NeedsPermission)
    );
    assert!(waiting.attention.as_ref().is_some_and(|event| event
        .event_id
        .starts_with(&format!("request:{}:", request.request_id.as_str()))));
    register_permission_responder(&server_requests, "task_1");
    answer_permission(&server_requests, "task_1", "allow");
    assert!(matches!(
        thread.join().unwrap().unwrap(),
        crate::agent::events::AgentPermissionOutcome::Selected { option_id } if option_id == "allow"
    ));
    assert!(store.read_task("task_1").unwrap().attention.is_none());
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

    sink.emit(agent_text_event("working")).unwrap();

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

    sink.emit(agent_text_event("working")).unwrap();

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
        server_requests.clone(),
    );

    sink.session_update(sourced_agent_text_event("before", "agent-message-1"))
        .unwrap();
    TaskTransitions::new(mutations, server_requests)
        .finish_turn("task_1", "turn_1", Ok(AgentPromptOutcome::EndTurn))
        .unwrap();

    sink.session_update(sourced_agent_text_event(" after", "agent-message-1"))
        .unwrap();

    let messages = store.read_messages("task_1").unwrap();
    let message = messages
        .iter()
        .find(|stored| agent_message_text(&stored.chat.message).is_some())
        .expect("Agent message is retained after prompt completion");
    assert_eq!(
        message.chat.identity,
        "acp:session_1:message:agent-message-1"
    );
    assert_eq!(message.chat.message_id, message.chat.identity);
    assert!(matches!(
        &message.chat.message,
        NormalizedMessage::AgentMessage { id, parts, .. }
            if id == &message.chat.identity
                && matches!(parts.as_slice(), [AgentMessagePart::Text { text }] if text == "before after")
    ));
}

#[test]
fn prompt_completion_leaves_running_agent_activity_open_for_later_session_updates() {
    let (_dir, store, mutations, server_requests) = test_runtime();
    store.write_task(&running_task("task_1")).unwrap();
    let sink = TaskSessionEventSink::new(
        mutations.clone(),
        "task_1".to_string(),
        "session_1".to_string(),
        server_requests.clone(),
    );
    sink.session_update(AgentEvent::ToolCall(AgentToolCall {
        tool_call_id: "tool_1".to_string(),
        scope_id: None,
        title: "Editing".to_string(),
        kind: "edit".to_string(),
        status: AgentToolCallStatus::InProgress,
        input_summary: None,
        output_preview: None,
        details: None,
    }))
    .unwrap();

    TaskTransitions::new(mutations, server_requests)
        .finish_turn("task_1", "turn_1", Ok(AgentPromptOutcome::EndTurn))
        .unwrap();

    let messages = store.read_messages("task_1").unwrap();
    assert!(messages.iter().any(|stored| matches!(
        stored.chat.message,
        NormalizedMessage::Activity {
            status: ActivityStatus::Running,
            ..
        }
    )));
    let task = store.read_task("task_1").unwrap();
    assert_eq!(task.status, TaskStatus::Inactive);
    assert_eq!(task.active_turn_id, None);
    assert_eq!(
        task.attention.as_ref().map(|event| event.reason),
        Some(TaskAttentionReason::Finished)
    );
}

#[test]
fn terminal_only_tool_updates_are_durable_without_task_revision() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().to_path_buf()).unwrap();
    let (notifier, notifications) = TaskUpdateNotifier::channel();
    let mutations = TaskMutations::new(
        store.clone(),
        Arc::new(Mutex::new(())),
        Arc::new(Mutex::new(RuntimeState::with_revision(0))),
        notifier,
    );
    let server_requests = ServerRequestRuntime::new();
    store.write_task(&running_task("task_terminal")).unwrap();
    let sink = TaskSessionEventSink::new(
        mutations,
        "task_terminal".to_string(),
        "session_1".to_string(),
        server_requests,
    );

    for _ in 0..100 {
        sink.session_update(AgentEvent::ToolUpdate(AgentToolUpdate {
            summary: None,
            terminal_appends: vec![AgentTerminalAppend {
                tool_call_id: "tool_1".to_string(),
                terminal_id: "terminal_1".to_string(),
                data: "x".to_string(),
            }],
        }))
        .unwrap();
    }
    let published = notifications
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("terminal-only durability publishes without a later Task mutation");
    assert!(matches!(
        published.kind,
        TaskUpdateKind::ToolDetailChanged { ref deltas, .. } if !deltas.is_empty()
    ));
    store.compact_message_journal("task_terminal").unwrap();

    let artifact_id =
        crate::storage::tool_artifacts::tool_artifact_id("acp_tool:session_1:tool_1", 0);
    let artifact = store
        .task_journal()
        .load_tool_artifact("task_terminal", &artifact_id)
        .unwrap();
    assert_eq!(artifact.terminal_outputs["terminal_1"], "x".repeat(100));
    assert_eq!(store.read_task("task_terminal").unwrap().revision, 0);
}

#[test]
fn terminal_append_immediately_before_task_change_is_published_once() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().to_path_buf()).unwrap();
    let (notifier, notifications) = TaskUpdateNotifier::channel();
    let mutations = TaskMutations::new(
        store.clone(),
        Arc::new(Mutex::new(())),
        Arc::new(Mutex::new(RuntimeState::with_revision(0))),
        notifier,
    );
    store
        .write_task(&running_task("task_terminal_barrier"))
        .unwrap();
    let sink = TaskSessionEventSink::new(
        mutations,
        "task_terminal_barrier".to_string(),
        "session_1".to_string(),
        ServerRequestRuntime::new(),
    );

    sink.session_update(AgentEvent::ToolUpdate(AgentToolUpdate {
        summary: None,
        terminal_appends: vec![AgentTerminalAppend {
            tool_call_id: "tool_1".to_string(),
            terminal_id: "terminal_1".to_string(),
            data: "before barrier".to_string(),
        }],
    }))
    .unwrap();
    sink.metadata_changed(AgentSessionMetadataUpdate {
        title: AgentMetadataField::Value("Changed title".to_string()),
        updated_at: AgentMetadataField::Unchanged,
    })
    .unwrap();

    let updates = (0..2)
        .map(|_| {
            notifications
                .recv_timeout(std::time::Duration::from_secs(1))
                .expect("terminal and Task changes are both published")
                .kind
        })
        .collect::<Vec<_>>();
    assert_eq!(
        updates
            .iter()
            .filter(|kind| matches!(kind, TaskUpdateKind::ToolDetailChanged { .. }))
            .count(),
        1
    );
    assert_eq!(
        updates
            .iter()
            .filter(|kind| matches!(kind, TaskUpdateKind::Changed(_)))
            .count(),
        1
    );
    assert!(notifications.try_recv().is_err());

    let artifact_id =
        crate::storage::tool_artifacts::tool_artifact_id("acp_tool:session_1:tool_1", 0);
    let artifact = store
        .task_journal()
        .load_tool_artifact("task_terminal_barrier", &artifact_id)
        .unwrap();
    assert_eq!(artifact.terminal_outputs["terminal_1"], "before barrier");
}

#[test]
fn mixed_tool_update_publishes_one_atomic_detail_delta() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().to_path_buf()).unwrap();
    let (notifier, notifications) = TaskUpdateNotifier::channel();
    let mutations = TaskMutations::new(
        store.clone(),
        Arc::new(Mutex::new(())),
        Arc::new(Mutex::new(RuntimeState::with_revision(0))),
        notifier,
    );
    store.write_task(&running_task("task_mixed_tool")).unwrap();
    let sink = TaskSessionEventSink::new(
        mutations,
        "task_mixed_tool".to_string(),
        "session_1".to_string(),
        ServerRequestRuntime::new(),
    );

    sink.session_update(AgentEvent::ToolUpdate(AgentToolUpdate {
        summary: Some(AgentToolCall {
            tool_call_id: "tool_1".to_string(),
            scope_id: None,
            title: "Running".to_string(),
            kind: "execute".to_string(),
            status: AgentToolCallStatus::InProgress,
            input_summary: None,
            output_preview: None,
            details: Some(Box::new(ActivityToolDetails {
                locations: Vec::new(),
                content: Vec::new(),
                input: None,
                output: None,
            })),
        }),
        terminal_appends: vec![AgentTerminalAppend {
            tool_call_id: "tool_1".to_string(),
            terminal_id: "terminal_1".to_string(),
            data: "complete".to_string(),
        }],
    }))
    .unwrap();

    let changed = (0..2)
        .find_map(|_| {
            let update = notifications
                .recv_timeout(std::time::Duration::from_secs(1))
                .expect("mixed update publication");
            match update.kind {
                TaskUpdateKind::Changed(change) => Some(change),
                TaskUpdateKind::ToolDetailChanged { .. } => None,
                TaskUpdateKind::HistorySync(_)
                | TaskUpdateKind::NavigationProjectEntriesChanged { .. }
                | TaskUpdateKind::NavigationRefreshStateChanged { .. } => None,
            }
        })
        .expect("Task change publication");
    assert_eq!(changed.tool_details.len(), 1);
    assert_eq!(changed.tool_details[0].details.revision, 1);
    assert_eq!(
        changed.tool_details[0].terminal_appends,
        vec![crate::storage::task_journal::TerminalOutputAppend {
            terminal_id: "terminal_1".to_string(),
            data: "complete".to_string(),
        }]
    );
}

#[test]
fn preceding_stream_and_mixed_update_share_one_atomic_tool_delta() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().to_path_buf()).unwrap();
    let (notifier, notifications) = TaskUpdateNotifier::channel();
    let mutations = TaskMutations::new(
        store.clone(),
        Arc::new(Mutex::new(())),
        Arc::new(Mutex::new(RuntimeState::with_revision(0))),
        notifier,
    );
    store
        .write_task(&running_task("task_coalesced_tool"))
        .unwrap();
    let sink = TaskSessionEventSink::new(
        mutations,
        "task_coalesced_tool".to_string(),
        "session_1".to_string(),
        ServerRequestRuntime::new(),
    );

    sink.session_update(AgentEvent::ToolUpdate(AgentToolUpdate {
        summary: None,
        terminal_appends: vec![AgentTerminalAppend {
            tool_call_id: "tool_1".to_string(),
            terminal_id: "terminal_1".to_string(),
            data: "before".to_string(),
        }],
    }))
    .unwrap();
    sink.session_update(AgentEvent::ToolUpdate(AgentToolUpdate {
        summary: Some(AgentToolCall {
            tool_call_id: "tool_1".to_string(),
            scope_id: None,
            title: "Finished".to_string(),
            kind: "execute".to_string(),
            status: AgentToolCallStatus::Completed,
            input_summary: None,
            output_preview: None,
            details: Some(Box::new(ActivityToolDetails {
                locations: Vec::new(),
                content: Vec::new(),
                input: None,
                output: None,
            })),
        }),
        terminal_appends: vec![AgentTerminalAppend {
            tool_call_id: "tool_1".to_string(),
            terminal_id: "terminal_1".to_string(),
            data: " after".to_string(),
        }],
    }))
    .unwrap();

    let update = notifications
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("one atomic mixed Tool update");
    let TaskUpdateKind::Changed(changed) = update.kind else {
        panic!("structured Tool publisher must own the coalesced artifact delta");
    };
    assert_eq!(changed.tool_details.len(), 1);
    assert_eq!(
        changed.tool_details[0]
            .terminal_appends
            .iter()
            .map(|append| append.data.as_str())
            .collect::<Vec<_>>(),
        vec!["before", " after"]
    );
    assert!(notifications.try_recv().is_err());

    let artifact_id =
        crate::storage::tool_artifacts::tool_artifact_id("acp_tool:session_1:tool_1", 0);
    let artifact = store
        .task_journal()
        .load_tool_artifact("task_coalesced_tool", &artifact_id)
        .unwrap();
    assert_eq!(artifact.terminal_outputs["terminal_1"], "before after");
}

#[test]
fn prompt_limit_appends_one_failed_activity_without_closing_agent_activity() {
    let (_dir, store, mutations, server_requests) = test_runtime();
    store.write_task(&running_task("task_1")).unwrap();
    let sink = TaskSessionEventSink::new(
        mutations.clone(),
        "task_1".to_string(),
        "session_1".to_string(),
        server_requests.clone(),
    );
    sink.session_update(AgentEvent::ToolCall(AgentToolCall {
        tool_call_id: "tool_1".to_string(),
        scope_id: None,
        title: "Editing".to_string(),
        kind: "edit".to_string(),
        status: AgentToolCallStatus::InProgress,
        input_summary: None,
        output_preview: None,
        details: None,
    }))
    .unwrap();

    TaskTransitions::new(mutations, server_requests)
        .finish_turn("task_1", "turn_1", Ok(AgentPromptOutcome::MaxTokens))
        .unwrap();

    let messages = store.read_messages("task_1").unwrap();
    assert_eq!(
        messages
            .iter()
            .filter(|stored| matches!(
                stored.chat.message,
                NormalizedMessage::Activity {
                    status: ActivityStatus::Running,
                    ..
                }
            ))
            .count(),
        1
    );
    let failures = messages
        .iter()
        .filter_map(|stored| match &stored.chat.message {
            NormalizedMessage::Activity {
                title,
                status: ActivityStatus::Error,
                steps,
                ..
            } => Some((title, steps)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].0, "Agent stopped");
    assert!(matches!(
        failures[0].1.as_slice(),
        [crate::protocol::model::ActivityStep::Text { text, level }]
            if text == "The Agent reached its token limit."
                && level.as_deref() == Some("error")
    ));
    let task = store.read_task("task_1").unwrap();
    assert_eq!(task.status, TaskStatus::Inactive);
    assert_eq!(task.active_turn_id, None);
    assert_eq!(
        task.attention.as_ref().map(|event| event.reason),
        Some(TaskAttentionReason::Stopped)
    );
}

#[test]
fn agent_confirmed_cancellation_ends_work_without_adding_chat() {
    let (_dir, store, mutations, server_requests) = test_runtime();
    store.write_task(&running_task("task_1")).unwrap();

    TaskTransitions::new(mutations, server_requests)
        .finish_turn("task_1", "turn_1", Ok(AgentPromptOutcome::Cancelled))
        .unwrap();

    assert!(store.read_messages("task_1").unwrap().is_empty());
    let task = store.read_task("task_1").unwrap();
    assert_eq!(task.status, TaskStatus::Inactive);
    assert_eq!(task.active_turn_id, None);
    assert_eq!(
        task.attention.as_ref().map(|event| event.reason),
        Some(TaskAttentionReason::Stopped)
    );
}

#[test]
fn user_stopped_turn_does_not_create_attention_event() {
    let (_dir, store, mutations, server_requests) = test_runtime();
    store.write_task(&running_task("task_1")).unwrap();
    let transitions = TaskTransitions::new(mutations, server_requests);
    assert!(transitions.mark_turn_stopping("task_1", "turn_1").unwrap());

    transitions
        .finish_turn("task_1", "turn_1", Ok(AgentPromptOutcome::Cancelled))
        .unwrap();

    let task = store.read_task("task_1").unwrap();
    assert_eq!(task.status, TaskStatus::Inactive);
    assert!(task.attention.is_none());
}

#[test]
fn cancellation_failure_returns_task_to_idle_and_closes_transient_requests() {
    let (_dir, store, mutations, server_requests) = test_runtime();
    store.write_task(&running_task("task_1")).unwrap();
    server_requests
        .open_task_secret_read_request(
            openaide_app_server_protocol::ids::TaskId::from("task_1"),
            "agent.secret".to_string(),
            Some("Agent secret".to_string()),
            crate::client_lifecycle::AppServerTime(1),
        )
        .unwrap();
    let transitions = TaskTransitions::new(mutations, server_requests.clone());
    assert!(transitions.mark_turn_stopping("task_1", "turn_1").unwrap());

    transitions
        .finish_turn(
            "task_1",
            "turn_1",
            Err(crate::protocol::errors::RuntimeError::NotReady(
                "cancel channel closed".to_string(),
            )),
        )
        .unwrap();

    let task = store.read_task("task_1").unwrap();
    assert_eq!(task.status, TaskStatus::Inactive);
    assert_eq!(task.active_turn_id, None);
    assert_eq!(
        task.attention.as_ref().map(|event| event.reason),
        Some(TaskAttentionReason::Failed)
    );
    let messages = store.read_messages("task_1").unwrap();
    assert!(messages.iter().any(|stored| matches!(
        &stored.chat.message,
        NormalizedMessage::Interruption {
            reason: crate::protocol::model::InterruptionReason::Failed,
            message,
            recoverable: true,
            ..
        } if message.contains("Unable to stop the Agent")
    )));
    assert!(server_requests
        .pending_for_task(&openaide_app_server_protocol::ids::TaskId::from("task_1"))
        .is_empty());
}

#[test]
fn agent_failure_interrupts_running_tools_and_returns_task_to_idle() {
    let (_dir, store, mutations, server_requests) = test_runtime();
    store.write_task(&running_task("task_1")).unwrap();
    let sink = TaskSessionEventSink::new(
        mutations.clone(),
        "task_1".to_string(),
        "session_1".to_string(),
        server_requests.clone(),
    );
    sink.session_update(AgentEvent::ToolCall(AgentToolCall {
        tool_call_id: "tool_1".to_string(),
        scope_id: None,
        title: "Editing".to_string(),
        kind: "edit".to_string(),
        status: AgentToolCallStatus::InProgress,
        input_summary: None,
        output_preview: None,
        details: None,
    }))
    .unwrap();

    TaskTransitions::new(mutations, server_requests)
        .finish_turn(
            "task_1",
            "turn_1",
            Err(crate::protocol::errors::RuntimeError::NotReady(
                "provider-internal-secret: Agent process exited".to_string(),
            )),
        )
        .unwrap();

    let task = store.read_task("task_1").unwrap();
    assert_eq!(task.status, TaskStatus::Inactive);
    assert_eq!(task.active_turn_id, None);
    assert_eq!(
        task.attention.as_ref().map(|event| event.reason),
        Some(TaskAttentionReason::Failed)
    );
    let messages = store.read_messages("task_1").unwrap();
    assert!(messages.iter().any(|stored| matches!(
        stored.chat.message,
        NormalizedMessage::Activity {
            status: ActivityStatus::Interrupted,
            ..
        }
    )));
    assert!(messages.iter().any(|stored| matches!(
        &stored.chat.message,
        NormalizedMessage::Interruption {
            reason: crate::protocol::model::InterruptionReason::Failed,
            message,
            recoverable: true,
            ..
        } if message == "Agent work stopped unexpectedly. Try again."
    )));
    assert!(messages.iter().all(|stored| {
        !format!("{:?}", stored.chat.message).contains("provider-internal-secret")
    }));
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

    sink.emit(agent_text_event("first")).unwrap();
    let appended = notifications.recv().unwrap();
    let message_id = match appended.kind {
        TaskUpdateKind::Changed(change) => match change.changes.chat.as_slice() {
            [TaskChatChange::Append { item }] => {
                assert_eq!(
                    item.status,
                    openaide_app_server_protocol::snapshot::ChatItemStatus::Complete
                );
                item.message_id.clone()
            }
            other => panic!("expected append, got {other:?}"),
        },
        other => panic!("expected Task change, got {other:?}"),
    };
    assert_eq!(store.read_messages("task_1").unwrap().len(), 1);

    sink.emit(agent_text_event(" second")).unwrap();
    let chunked = notifications.recv().unwrap();
    assert!(matches!(
        chunked.kind,
        TaskUpdateKind::Changed(change)
            if matches!(change.changes.chat.as_slice(),
                [TaskChatChange::AppendText { message_id: id, text }]
                    if id == &message_id && text == " second")
    ));
    let stored = store.read_messages("task_1").unwrap();
    assert!(matches!(
        &stored[0].chat.message,
        message if agent_message_text(message) == Some("first second")
    ));

    sink.emit(AgentEvent::Activity {
        title: "Tool".to_string(),
        tool_name: "fixture".to_string(),
        output_preview: "done".to_string(),
    })
    .unwrap();
    let activity_update = notifications.recv().unwrap();
    assert!(matches!(
        activity_update.kind,
        TaskUpdateKind::Changed(change)
            if matches!(change.changes.chat.as_slice(), [TaskChatChange::Append { .. }])
    ));
}

#[test]
fn every_tool_update_commits_one_lightweight_upsert_and_latest_detail() {
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
    let sink = TaskSessionEventSink::new(
        mutations,
        "task_1".to_string(),
        "session_1".to_string(),
        ServerRequestRuntime::new(),
    );

    for output in ["first", "latest"] {
        sink.session_update(AgentEvent::ToolCall(AgentToolCall {
            tool_call_id: "tool_1".to_string(),
            scope_id: None,
            title: "Run checks".to_string(),
            kind: "execute".to_string(),
            status: AgentToolCallStatus::InProgress,
            input_summary: Some("cargo test".to_string()),
            output_preview: Some(output.to_string()),
            details: Some(Box::new(ActivityToolDetails {
                locations: Vec::new(),
                content: Vec::new(),
                input: None,
                output: Some(ActivityToolOutput {
                    stdout: Some(output.to_string()),
                    stderr: None,
                    formatted_output: None,
                    aggregated_output: None,
                    exit_code: None,
                    success: None,
                    fields: Vec::new(),
                }),
            })),
        }))
        .unwrap();

        let update = notifications.recv().unwrap();
        match update.kind {
            TaskUpdateKind::Changed(change) => {
                let [TaskChatChange::Upsert { item }] = change.changes.chat.as_slice() else {
                    panic!("expected focused Tool upsert: {:?}", change.changes.chat);
                };
                assert_eq!(item.message_id.as_str(), "acp_tool:session_1:tool_1");
                assert!(item.parts.iter().all(|part| !matches!(
                    part,
                    openaide_app_server_protocol::snapshot::MessagePart::Activity { steps, .. }
                        if steps.iter().any(|step| matches!(
                            step,
                            openaide_app_server_protocol::snapshot::ActivityStepSnapshot::Tool {
                                details: Some(_), ..
                            }
                        ))
                )));
                assert_eq!(change.tool_details.len(), 1);
                assert_eq!(
                    change.tool_details[0].artifact_id,
                    crate::storage::tool_artifacts::tool_artifact_id(
                        "acp_tool:session_1:tool_1",
                        0,
                    )
                );
                assert_eq!(
                    change.tool_details[0]
                        .details
                        .output
                        .as_ref()
                        .and_then(|detail| detail.stdout.as_deref()),
                    Some(output)
                );
            }
            other => panic!("expected Task change, got {other:?}"),
        }
    }

    assert_eq!(store.read_messages("task_1").unwrap().len(), 1);
    let artifact_id =
        crate::storage::tool_artifacts::tool_artifact_id("acp_tool:session_1:tool_1", 0);
    assert_eq!(
        store
            .read_tool_artifact("task_1", &artifact_id)
            .unwrap()
            .output
            .and_then(|detail| detail.stdout),
        Some("latest".to_string())
    );
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
        sink.emit(sourced_agent_text_event(text, source_message_id))
            .unwrap();
    }

    let messages = store.read_messages("task_1").unwrap();
    let agent_texts = messages
        .iter()
        .filter_map(|stored| agent_message_text(&stored.chat.message))
        .collect::<Vec<_>>();
    assert_eq!(agent_texts, ["A1A2", "B1B2"]);
}

#[test]
fn sourced_mixed_content_updates_one_ordered_chat_message() {
    let (_dir, store, mutations, server_requests) = test_runtime();
    store.write_task(&running_task("task_1")).unwrap();
    let sink = TaskEventSink::new(
        mutations,
        "task_1".to_string(),
        "turn_1".to_string(),
        server_requests,
        TurnCancellation::new(),
    );

    sink.emit(sourced_agent_text_event("Before", "message-1"))
        .unwrap();
    sink.emit(AgentEvent::MessageChunk {
        role: AgentMessageRole::Agent,
        part: AgentMessagePart::Resource {
            uri: "file:///result.txt".to_string(),
            name: Some("result.txt".to_string()),
            title: None,
            description: None,
            media_type: Some("text/plain".to_string()),
            size_bytes: None,
            text: Some("Result".to_string()),
        },
        source_message_id: Some("message-1".to_string()),
    })
    .unwrap();
    sink.emit(sourced_agent_text_event("After", "message-1"))
        .unwrap();

    let messages = store.read_messages("task_1").unwrap();
    assert_eq!(messages.len(), 1);
    assert!(matches!(
        &messages[0].chat.message,
        NormalizedMessage::AgentMessage { parts, .. }
            if matches!(parts.as_slice(), [
                AgentMessagePart::Text { text: before },
                AgentMessagePart::Resource { uri, .. },
                AgentMessagePart::Text { text: after },
            ] if before == "Before" && uri == "file:///result.txt" && after == "After")
    ));
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

    sink.emit(agent_text_event("complete me")).unwrap();
    let _appended = notifications.recv().unwrap();
    assert!(matches!(
        &store.read_messages("task_1").unwrap()[0].chat.message,
        NormalizedMessage::AgentMessage { .. }
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

    sink.emit(agent_text_event("before permission")).unwrap();
    sink.emit(tool_event(AgentToolCallStatus::Pending)).unwrap();
    let permission_sink = sink.clone();
    let permission_thread = std::thread::spawn(move || {
        permission_sink.request_permission(permission_request("permission_1"))
    });
    while server_requests.pending_count() == 0 {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    answer_permission(&server_requests, "task_1", "allow");
    permission_thread
        .join()
        .expect("permission thread joins")
        .unwrap();

    sink.emit(agent_text_event(" after permission")).unwrap();

    let messages = store.read_messages("task_1").unwrap();
    let agent_text: Vec<_> = messages
        .iter()
        .filter_map(|stored| agent_message_text(&stored.chat.message))
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

    sink.emit(tool_event(AgentToolCallStatus::Pending)).unwrap();
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
        let result = event_sink.emit(agent_text_event("while waiting"));
        let _ = emit_done_tx.send(result);
    });
    let emitted_while_waiting = emit_done_rx
        .recv_timeout(std::time::Duration::from_millis(250))
        .is_ok();

    answer_permission(&server_requests, "task_1", "allow");
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
            message if agent_message_text(message) == Some("while waiting")
        )
    }));
}

#[test]
fn finishing_active_turn_marks_task_unread_for_user_attention() {
    let (_dir, store, mutations, server_requests) = test_runtime();
    store.write_task(&running_task("task_1")).unwrap();
    let transitions = TaskTransitions::new(mutations, server_requests);

    assert!(transitions
        .finish_turn("task_1", "turn_1", Ok(AgentPromptOutcome::EndTurn))
        .unwrap());

    let stored = store.read_task("task_1").unwrap();
    assert!(stored.unread);
    assert_eq!(
        stored.attention.as_ref().map(|event| event.reason),
        Some(TaskAttentionReason::Finished)
    );
    assert!(!stored.attention.as_ref().unwrap().event_id.is_empty());
    assert!(!stored.attention.as_ref().unwrap().occurred_at.is_empty());
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
        options: vec![
            AgentPermissionOption {
                option_id: "allow".to_string(),
                name: "Allow".to_string(),
                kind: AgentPermissionOptionKind::AllowOnce,
            },
            AgentPermissionOption {
                option_id: "reject".to_string(),
                name: "Reject".to_string(),
                kind: AgentPermissionOptionKind::RejectOnce,
            },
        ],
    }
}

fn tool_event(status: AgentToolCallStatus) -> AgentEvent {
    AgentEvent::ToolCall(AgentToolCall {
        tool_call_id: "tool_1".to_string(),
        scope_id: None,
        title: "Editing".to_string(),
        kind: "edit".to_string(),
        status,
        input_summary: None,
        output_preview: None,
        details: None,
    })
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
        attention: None,
        created_at: "1".to_string(),
        updated_at: "1".to_string(),
        last_activity: "1".to_string(),
        agent_name: "Codex".to_string(),
        agent_id: "codex".to_string(),
        isolation: IsolationKind::Local,
        workspace_root: "/tmp/workspace".to_string(),
        project_root: None,
        worktree_id: None,
        lifecycle: crate::storage::records::TaskLifecycle::Open,
        agent_session_id: Some("session_1".to_string()),
        active_turn_id: Some("turn_1".to_string()),
        active_turn_started_at: None,
        tombstoned: false,
        revision: 0,
        config_options_catalog: None,
        config_mutation: Default::default(),
        agent_commands_catalog: None,
        model_id: None,
        supports_image_input: false,
        preparation: TaskPreparationRecord::Ready,
    }
}
