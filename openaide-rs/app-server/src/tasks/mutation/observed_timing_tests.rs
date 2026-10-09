use crate::protocol::model::{
    ActivityStatus, ActivityStep, AgentMessagePart, AgentMessageRole, ChatTiming,
    InterruptionReason, NormalizedMessage, ObservedSpan, TaskStatus,
};
use crate::storage::Store;
use crate::tasks::mutation::tests::{task_record, test_mutations};
use crate::tasks::mutation::{TaskCommitOptions, TaskMutationContext, TaskMutationResult};

const TASK: &str = "task_timing";

fn commit(
    mutations: &crate::tasks::mutation::TaskMutations,
    change: impl FnOnce(&mut TaskMutationContext<'_>),
) {
    mutations
        .commit_existing_task(TASK, TaskCommitOptions::metadata(), |ctx| {
            change(ctx);
            Ok(TaskMutationResult::Changed)
        })
        .unwrap();
}

fn timing_of(store: &Store, identity: &str) -> ChatTiming {
    store
        .read_messages(TASK)
        .unwrap()
        .into_iter()
        .find(|stored| stored.chat.identity == identity)
        .unwrap_or_else(|| panic!("row {identity} is stored"))
        .chat
        .timing
}

fn user(id: &str, created_at: &str) -> NormalizedMessage {
    NormalizedMessage::User {
        id: id.to_string(),
        text: "prompt".to_string(),
        created_at: created_at.to_string(),
        attachments: Vec::new(),
    }
}

fn answer(id: &str, created_at: &str) -> NormalizedMessage {
    NormalizedMessage::AgentMessage {
        id: id.to_string(),
        role: AgentMessageRole::Agent,
        parts: vec![AgentMessagePart::Text {
            text: "answer".to_string(),
        }],
        created_at: created_at.to_string(),
    }
}

fn tool(id: &str, status: ActivityStatus, created_at: &str) -> NormalizedMessage {
    NormalizedMessage::Activity {
        id: id.to_string(),
        title: "Read".to_string(),
        status,
        created_at: created_at.to_string(),
        collapsed: true,
        steps: vec![ActivityStep::Tool {
            background_outcome: None,
            tool_call_id: Some(id.to_string()),
            name: "read".to_string(),
            status,
            presentation: None,
            description: None,
            input_summary: None,
            output_preview: None,
            detail_artifact_id: None,
            details: None,
            permission_outcomes: Vec::new(),
        }],
    }
}

fn ended_after(span: &ObservedSpan) -> bool {
    let started: u128 = span.started_at.parse().unwrap();
    span.ended_at
        .as_deref()
        .is_some_and(|ended| ended.parse::<u128>().unwrap() >= started)
}

#[test]
fn a_live_user_message_records_when_it_was_sent() {
    let (_dir, store, mutations, _notifications) = test_mutations(0);
    store.write_task(&task_record(TASK)).unwrap();

    commit(&mutations, |ctx| {
        ctx.append_message(user("u1", "40")).unwrap()
    });

    assert_eq!(timing_of(&store, "u1").sent_at.as_deref(), Some("40"));
}

#[test]
fn a_tool_seen_running_is_timed_from_its_start_to_its_settling_update() {
    let (_dir, store, mutations, _notifications) = test_mutations(0);
    store.write_task(&task_record(TASK)).unwrap();

    commit(&mutations, |ctx| {
        ctx.upsert_message_with_details(tool("t1", ActivityStatus::Running, "50"))
            .unwrap();
    });
    let running = timing_of(&store, "t1")
        .run
        .expect("running tool has a start");
    assert_eq!(running.started_at, "50");
    assert_eq!(running.ended_at, None);

    commit(&mutations, |ctx| {
        ctx.upsert_message_with_details(tool("t1", ActivityStatus::Completed, "99"))
            .unwrap();
    });
    let settled = timing_of(&store, "t1")
        .run
        .expect("settled tool keeps its run");
    assert_eq!(settled.started_at, "50");
    assert!(ended_after(&settled));
}

#[test]
fn a_tool_first_seen_settled_stays_untimed() {
    let (_dir, store, mutations, _notifications) = test_mutations(0);
    store.write_task(&task_record(TASK)).unwrap();

    commit(&mutations, |ctx| {
        ctx.upsert_message_with_details(tool("t1", ActivityStatus::Completed, "50"))
            .unwrap();
    });
    // A later running report cannot recover a start nobody witnessed.
    commit(&mutations, |ctx| {
        ctx.upsert_message_with_details(tool("t1", ActivityStatus::Running, "60"))
            .unwrap();
    });

    assert_eq!(timing_of(&store, "t1"), ChatTiming::default());
}

#[test]
fn interrupting_running_work_closes_its_run() {
    let (_dir, store, mutations, _notifications) = test_mutations(0);
    store.write_task(&task_record(TASK)).unwrap();
    commit(&mutations, |ctx| {
        ctx.upsert_message_with_details(tool("t1", ActivityStatus::Running, "50"))
            .unwrap();
    });

    commit(&mutations, |ctx| {
        ctx.finish_running_activities(ActivityStatus::Interrupted)
            .unwrap();
    });

    assert!(ended_after(&timing_of(&store, "t1").run.unwrap()));
}

#[test]
fn a_finished_turn_is_timed_on_its_own_final_answer() {
    let (_dir, store, mutations, _notifications) = test_mutations(0);
    store.write_task(&task_record(TASK)).unwrap();
    commit(&mutations, |ctx| {
        ctx.append_message(user("u1", "10")).unwrap();
        ctx.append_message(answer("a1", "11")).unwrap();
        ctx.append_message(user("u2", "20")).unwrap();
        ctx.append_message(answer("a2", "21")).unwrap();
        ctx.append_message(tool("t1", ActivityStatus::Completed, "22"))
            .unwrap();
        ctx.append_message(answer("a3", "23")).unwrap();
        ctx.task_mut().status = TaskStatus::Active;
        ctx.task_mut().active_turn_started_at = Some("20".to_string());
    });

    commit(&mutations, |ctx| {
        ctx.close_turn_on_final_answer("260").unwrap();
    });

    assert_eq!(
        timing_of(&store, "a3").closed_turn,
        Some(ObservedSpan {
            started_at: "20".to_string(),
            ended_at: Some("260".to_string()),
        })
    );
    assert_eq!(timing_of(&store, "a2").closed_turn, None);
    assert_eq!(timing_of(&store, "a1").closed_turn, None);
}

#[test]
fn a_turn_without_an_answer_times_no_earlier_row() {
    let (_dir, store, mutations, _notifications) = test_mutations(0);
    store.write_task(&task_record(TASK)).unwrap();
    commit(&mutations, |ctx| {
        ctx.append_message(user("u1", "10")).unwrap();
        ctx.append_message(answer("a1", "11")).unwrap();
        ctx.append_message(user("u2", "20")).unwrap();
        ctx.task_mut().active_turn_started_at = Some("20".to_string());
    });

    commit(&mutations, |ctx| {
        ctx.close_turn_on_final_answer("260").unwrap();
    });

    assert_eq!(timing_of(&store, "a1").closed_turn, None);
}

#[test]
fn an_interrupted_turn_is_timed_on_its_interruption() {
    let (_dir, store, mutations, _notifications) = test_mutations(0);
    store.write_task(&task_record(TASK)).unwrap();
    commit(&mutations, |ctx| {
        ctx.append_message(user("u1", "10")).unwrap();
        ctx.task_mut().active_turn_started_at = Some("10".to_string());
    });

    commit(&mutations, |ctx| {
        ctx.append_message(NormalizedMessage::Interruption {
            id: "stop".to_string(),
            reason: InterruptionReason::Canceled,
            message: "Task was stopped.".to_string(),
            created_at: "90".to_string(),
            recoverable: true,
        })
        .unwrap();
        ctx.close_turn_on_last_row("90").unwrap();
    });

    assert_eq!(
        timing_of(&store, "stop").closed_turn,
        Some(ObservedSpan {
            started_at: "10".to_string(),
            ended_at: Some("90".to_string()),
        })
    );
}

#[test]
fn reloaded_native_history_is_untimed_except_for_rows_already_observed() {
    let (_dir, store, mutations, _notifications) = test_mutations(0);
    store.write_task(&task_record(TASK)).unwrap();
    commit(&mutations, |ctx| {
        ctx.append_message(user("u1", "40")).unwrap()
    });

    commit(&mutations, |ctx| {
        ctx.replace_messages_from_native_session(
            vec![
                user("u1", "500"),
                tool("replayed", ActivityStatus::Running, "500"),
                user("u2", "500"),
            ],
            500,
        )
        .unwrap();
    });

    assert_eq!(timing_of(&store, "u1").sent_at.as_deref(), Some("40"));
    assert_eq!(timing_of(&store, "replayed"), ChatTiming::default());
    assert_eq!(timing_of(&store, "u2"), ChatTiming::default());
}
