use std::time::Duration;

use serde_json::json;

use super::*;

fn session_update(update: serde_json::Value) -> UntypedMessage {
    UntypedMessage::new(
        "session/update",
        json!({ "sessionId": "session-1", "update": update }),
    )
    .expect("untyped session update")
}

fn forwarded_state(notification: &UntypedMessage) -> Option<(String, String)> {
    match route_async_task_update(notification) {
        AsyncTaskRouting::Forward(message) => {
            assert_eq!(message.method(), ASYNC_TASK_STATE_METHOD);
            assert_eq!(message.params()["sessionId"], "session-1");
            Some((
                message.params()["asyncTaskId"].as_str()?.to_string(),
                message.params()["state"].as_str()?.to_string(),
            ))
        }
        AsyncTaskRouting::NotAsyncTask | AsyncTaskRouting::Drop => None,
    }
}

fn task_state(async_task_id: &str, state: AsyncTaskState) -> AsyncTaskStateNotification {
    AsyncTaskStateNotification {
        session_id: "session-1".to_string().into(),
        async_task_id: async_task_id.to_string(),
        state,
    }
}

fn agent_text() -> SessionUpdate {
    serde_json::from_value(json!({
        "sessionUpdate": "agent_message_chunk",
        "content": { "type": "text", "text": "The build passed." },
    }))
    .expect("agent message chunk")
}

fn turn_ended() -> TurnEndedNotification {
    serde_json::from_value(json!({ "sessionId": "session-1", "stopReason": "end_turn" }))
        .expect("turn ended notification")
}

#[test]
fn a_spawn_and_a_state_change_reach_the_session_worker() {
    let spawned = session_update(json!({
        "sessionUpdate": "async_task_spawned",
        "asyncTaskId": "task-1",
        "name": "npm test",
        "canStop": true,
    }));
    assert_eq!(
        forwarded_state(&spawned),
        Some(("task-1".to_string(), "running".to_string()))
    );

    let completed = session_update(json!({
        "sessionUpdate": "async_task_state_update",
        "asyncTaskId": "task-1",
        "state": "completed",
        "summary": "exit code 0",
    }));
    assert_eq!(
        forwarded_state(&completed),
        Some(("task-1".to_string(), "completed".to_string()))
    );
}

#[test]
fn updates_that_change_no_liveness_stop_at_the_connection() {
    let progress = session_update(json!({
        "sessionUpdate": "async_task_progress",
        "asyncTaskId": "task-1",
        "summary": "compiling",
    }));
    assert!(matches!(
        route_async_task_update(&progress),
        AsyncTaskRouting::Drop
    ));

    let unknown_state = session_update(json!({
        "sessionUpdate": "async_task_state_update",
        "asyncTaskId": "task-1",
        "state": "hibernating",
    }));
    assert!(matches!(
        route_async_task_update(&unknown_state),
        AsyncTaskRouting::Drop
    ));

    let missing_id = session_update(json!({ "sessionUpdate": "async_task_spawned" }));
    assert!(matches!(
        route_async_task_update(&missing_id),
        AsyncTaskRouting::Drop
    ));
}

#[test]
fn other_notifications_keep_their_typed_path() {
    let chunk = session_update(json!({
        "sessionUpdate": "agent_message_chunk",
        "content": { "type": "text", "text": "hello" },
    }));
    assert!(matches!(
        route_async_task_update(&chunk),
        AsyncTaskRouting::NotAsyncTask
    ));

    let other_method = UntypedMessage::new(
        "_session/turn_ended",
        json!({ "sessionId": "session-1", "update": { "sessionUpdate": "async_task_spawned" } }),
    )
    .expect("untyped notification");
    assert!(matches!(
        route_async_task_update(&other_method),
        AsyncTaskRouting::NotAsyncTask
    ));
}

#[test]
fn a_live_command_holds_the_turn_until_its_followup_cycle_ends() {
    let work = BackgroundWork::default();
    assert!(!work.holds_turn("session-1", "task-a"));

    work.task_state_changed(&task_state("task-1", AsyncTaskState::Running));
    assert!(work.holds_turn("session-1", "task-a"));
    assert!(work.is_holding());

    // The command is gone, but the model has not answered its result yet.
    work.task_state_changed(&task_state("task-1", AsyncTaskState::Completed));
    assert!(work.holds_turn("session-1", "task-a"));

    work.session_update_observed(&agent_text());
    assert!(work.holds_turn("session-1", "task-a"));

    work.turn_ended(&turn_ended());
    assert!(!work.holds_turn("session-1", "task-a"));
}

#[test]
fn a_command_that_ended_inside_the_prompt_does_not_hold_its_turn() {
    let work = BackgroundWork::default();
    work.task_state_changed(&task_state("task-1", AsyncTaskState::Running));
    work.task_state_changed(&task_state("task-1", AsyncTaskState::Completed));
    // Updates of the prompt itself are not a cycle of their own.
    work.session_update_observed(&agent_text());

    assert!(!work.holds_turn("session-1", "task-a"));
    assert!(!work.is_holding());
}

#[test]
fn a_stopped_command_promises_no_followup() {
    let work = BackgroundWork::default();
    work.task_state_changed(&task_state("task-1", AsyncTaskState::Running));
    assert!(work.holds_turn("session-1", "task-a"));

    work.task_state_changed(&task_state("task-1", AsyncTaskState::Stopped));
    assert!(!work.holds_turn("session-1", "task-a"));
}

#[test]
fn a_followup_that_never_shows_releases_the_turn() {
    // timing: expiry — a zero grace has already run out at the next check.
    let work = BackgroundWork::with_followup_grace(Duration::ZERO);
    work.task_state_changed(&task_state("task-1", AsyncTaskState::Running));
    assert!(work.holds_turn("session-1", "task-a"));

    work.task_state_changed(&task_state("task-1", AsyncTaskState::Completed));
    assert!(!work.holds_turn("session-1", "task-a"));
}

#[test]
fn a_continuation_prompt_takes_over_from_a_running_cycle() {
    let work = BackgroundWork::default();
    work.task_state_changed(&task_state("task-1", AsyncTaskState::Running));
    assert!(work.holds_turn("session-1", "task-a"));
    work.task_state_changed(&task_state("task-1", AsyncTaskState::Completed));
    work.session_update_observed(&agent_text());

    work.prompt_continued();
    assert!(!work.holds_turn("session-1", "task-a"));
}

#[test]
fn finishing_a_hold_keeps_live_commands_for_the_next_prompt() {
    let work = BackgroundWork::default();
    work.task_state_changed(&task_state("task-1", AsyncTaskState::Running));
    assert!(work.holds_turn("session-1", "task-a"));

    work.finish_hold("session-1", "task-a", "cancelled");
    assert!(!work.is_holding());
    assert!(work.holds_turn("session-1", "task-b"));
}
