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
        state: Some(state),
        description: Some(format!("run {async_task_id}")),
        kind_label: None,
        tool_call_id: None,
        can_stop: Some(true),
    }
}

fn live_ids(work: &BackgroundWork, turn_held: bool) -> (bool, Vec<String>) {
    let report = work.report(turn_held);
    let ids = report
        .commands
        .into_iter()
        .map(|command| command.command_id)
        .collect();
    (report.held, ids)
}

fn agent_state(state: &str) -> SessionStateNotification {
    serde_json::from_value(json!({ "sessionId": "session-1", "state": state }))
        .expect("session state notification")
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

    work.session_state_changed(&agent_state("running"));
    assert!(work.holds_turn("session-1", "task-a"));

    work.session_state_changed(&agent_state("idle"));
    assert!(!work.holds_turn("session-1", "task-a"));
}

#[test]
fn a_hold_is_background_only_until_the_agent_works_again() {
    let work = BackgroundWork::default();
    work.task_state_changed(&task_state("task-1", AsyncTaskState::Running));
    assert_eq!(
        live_ids(&work, false),
        (false, vec!["task-1".to_string()]),
        "the command is listed while the prompt itself is still running"
    );

    assert!(work.holds_turn("session-1", "task-a"));
    assert_eq!(live_ids(&work, true), (true, vec!["task-1".to_string()]));

    // The follow-up cycle is awaited with no command left to list.
    work.task_state_changed(&task_state("task-1", AsyncTaskState::Completed));
    assert_eq!(live_ids(&work, true), (true, Vec::new()));

    // A permission request inside the cycle is still the Agent working.
    work.session_state_changed(&agent_state("running"));
    work.session_state_changed(&agent_state("requires_action"));
    assert_eq!(live_ids(&work, true), (false, Vec::new()));

    // The cycle ended while a second command is still live.
    work.task_state_changed(&task_state("task-2", AsyncTaskState::Running));
    work.session_state_changed(&agent_state("idle"));
    assert_eq!(live_ids(&work, true), (true, vec!["task-2".to_string()]));
}

#[test]
fn a_command_carries_its_details_and_reports_how_it_ended_to_its_tool_row() {
    let spawned = session_update(json!({
        "sessionUpdate": "async_task_spawned",
        "asyncTaskId": "task-1",
        "name": "Background task",
        "description": "npm run build",
        "taskType": "shell",
        "canStop": true,
    }));
    let AsyncTaskRouting::Forward(message) = route_async_task_update(&spawned) else {
        panic!("a spawn is forwarded");
    };
    let spawned: AsyncTaskStateNotification =
        serde_json::from_value(message.params().clone()).expect("forwarded spawn");
    let work = BackgroundWork::default();
    work.task_state_changed(&spawned);
    let command = work.report(false).commands.remove(0);
    assert_eq!(command.description, "npm run build");
    assert_eq!(command.kind_label, None, "a shell command is unlabeled");
    assert!(command.can_stop);
    assert_eq!(command.tool_call_id, None);

    // The Tool call is correlated later, by a progress update.
    let progress = session_update(json!({
        "sessionUpdate": "async_task_progress",
        "asyncTaskId": "task-1",
        "toolCallId": "tool-7",
    }));
    let AsyncTaskRouting::Forward(message) = route_async_task_update(&progress) else {
        panic!("a correlating progress update is forwarded");
    };
    work.task_state_changed(
        &serde_json::from_value(message.params().clone()).expect("forwarded progress"),
    );
    assert_eq!(
        work.report(false).commands[0].tool_call_id.as_deref(),
        Some("tool-7")
    );
    assert!(matches!(
        route_async_task_update(&session_update(json!({
            "sessionUpdate": "async_task_progress",
            "asyncTaskId": "task-1",
            "summary": "still building",
        }))),
        AsyncTaskRouting::Drop
    ));

    assert!(work.take_finished().is_empty());
    work.task_state_changed(&AsyncTaskStateNotification {
        description: None,
        can_stop: None,
        ..task_state("task-1", AsyncTaskState::Failed)
    });
    assert!(work.report(false).commands.is_empty());
    assert_eq!(
        work.take_finished(),
        vec![FinishedCommand {
            tool_call_id: "tool-7".to_string(),
            outcome: BackgroundCommandOutcome::Failed,
        }]
    );
    assert!(work.take_finished().is_empty(), "an end is reported once");
}

#[test]
fn a_command_that_ended_inside_the_prompt_does_not_hold_its_turn() {
    let work = BackgroundWork::default();
    work.session_state_changed(&agent_state("running"));
    work.task_state_changed(&task_state("task-1", AsyncTaskState::Running));
    work.task_state_changed(&task_state("task-1", AsyncTaskState::Completed));
    // The response ends the prompt's work before the Agent reports `idle`.
    work.prompt_answered();

    assert!(!work.holds_turn("session-1", "task-a"));
    assert!(!work.is_holding());
}

#[test]
fn a_stopped_command_promises_no_followup() {
    let work = BackgroundWork::default();
    work.task_state_changed(&task_state("task-1", AsyncTaskState::Running));
    work.task_state_changed(&task_state("task-2", AsyncTaskState::Running));
    assert!(work.holds_turn("session-1", "task-a"));

    // The Agent acknowledges each stop in Chat and repeats `idle`; neither is
    // a cycle.
    work.task_state_changed(&task_state("task-1", AsyncTaskState::Stopped));
    work.session_state_changed(&agent_state("idle"));
    assert_eq!(live_ids(&work, true), (true, vec!["task-2".to_string()]));

    work.task_state_changed(&task_state("task-2", AsyncTaskState::Stopped));
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
fn a_command_that_finished_during_a_cycle_is_awaited_after_it() {
    let work = BackgroundWork::default();
    work.task_state_changed(&task_state("task-1", AsyncTaskState::Running));
    work.task_state_changed(&task_state("task-2", AsyncTaskState::Running));
    assert!(work.holds_turn("session-1", "task-a"));

    work.task_state_changed(&task_state("task-1", AsyncTaskState::Completed));
    work.session_state_changed(&agent_state("running"));
    // The second command ends while the first one's cycle is still running.
    work.task_state_changed(&task_state("task-2", AsyncTaskState::Completed));
    work.session_state_changed(&agent_state("idle"));
    assert_eq!(
        live_ids(&work, true),
        (true, Vec::new()),
        "its own cycle may still start"
    );

    work.session_state_changed(&agent_state("running"));
    work.session_state_changed(&agent_state("idle"));
    assert!(!work.holds_turn("session-1", "task-a"));
}

#[test]
fn a_continuation_prompt_takes_over_from_a_running_cycle() {
    let work = BackgroundWork::default();
    work.task_state_changed(&task_state("task-1", AsyncTaskState::Running));
    assert!(work.holds_turn("session-1", "task-a"));
    work.task_state_changed(&task_state("task-1", AsyncTaskState::Completed));
    work.session_state_changed(&agent_state("running"));

    // The prompt joins the cycle, so the Agent reports no new `running`.
    work.prompt_continued("session-1", "task-a");
    assert!(!work.is_holding());
    work.prompt_answered();
    assert!(!work.holds_turn("session-1", "task-a"));
}

#[test]
fn a_continuation_prompt_answering_during_a_hold_is_not_a_cycle() {
    let work = BackgroundWork::default();
    work.task_state_changed(&task_state("task-1", AsyncTaskState::Running));
    assert!(work.holds_turn("session-1", "task-a"));

    // The user steers the held turn; the Agent works on the answer.
    work.prompt_continued("session-1", "task-a");
    assert!(!work.is_holding());
    work.session_state_changed(&agent_state("running"));
    work.prompt_answered();

    // Its response finds the command still running: background again.
    assert!(work.holds_turn("session-1", "task-a"));
    assert_eq!(live_ids(&work, true), (true, vec!["task-1".to_string()]));

    // No cycle follows a stopped command, so nothing is left to wait for.
    work.session_state_changed(&agent_state("idle"));
    work.task_state_changed(&task_state("task-1", AsyncTaskState::Stopped));
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
