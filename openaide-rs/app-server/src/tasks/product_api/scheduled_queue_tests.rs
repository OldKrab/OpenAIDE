use super::*;
use openaide_app_server_protocol::task::TaskQueueResumeParams;

const DUE: &str = "4102444800000";
const BEFORE: &str = "4102444799999";
const TASK: &str = "scheduled-task";

struct Fixture {
    _root: tempfile::TempDir,
    api: TaskProductApi,
    store: Store,
    agent: Arc<RecordingAgent>,
}

impl Fixture {
    fn new(block_prompt: bool) -> Self {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path().join("state")).unwrap();
        store
            .write_task(&task_record(
                TASK,
                &root.path().join("workspace").to_string_lossy(),
            ))
            .unwrap();
        let agent = Arc::new(RecordingAgent {
            block_prompt,
            ..Default::default()
        });
        let api = TaskProductApi::new(
            store.clone(),
            Arc::new(StorageProjectResolver::new(store.clone())),
            AgentRegistry::default_built_ins(),
            agent.clone(),
            TaskUpdateNotifier::disabled(),
        )
        .unwrap();
        Self {
            _root: root,
            api,
            store,
            agent,
        }
    }

    fn schedule(&self, text: &str) -> TaskSnapshot {
        self.api
            .queue_append_for_test(TaskQueueAppendParams {
                task_id: TASK.into(),
                message: ComposerMessage {
                    text: Some(text.into()),
                    ..Default::default()
                },
                not_before: Some(DUE.into()),
            })
            .unwrap()
    }

    fn settled(&self) {
        wait_until(|| self.store.read_task(TASK).unwrap().status == TaskStatus::Inactive);
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.agent.release_prompt.store(true, Ordering::SeqCst);
        self.settled();
    }
}

#[test]
fn scheduled_queue_accepts_idle_work_and_delivers_once_at_the_deadline() {
    let f = Fixture::new(false);
    let accepted = f.schedule("scheduled instruction");
    assert_eq!(
        accepted.message_queue.items[0].not_before.as_deref(),
        Some(DUE)
    );
    assert!(accepted.chat.items.is_empty());
    let passive = f.api.session_operations.begin_passive(TASK);
    f.api.deliver_scheduled_queue_at(TASK, BEFORE);
    assert!(
        f.api
            .session_operations
            .try_serialize_passive(&passive, || ())
            .is_some(),
        "waiting for a future deadline must not invalidate passive session recovery"
    );
    assert_eq!(f.agent.prompts.load(Ordering::SeqCst), 0);
    assert_eq!(
        f.store.read_task(TASK).unwrap().message_queue.items.len(),
        1
    );

    f.api.deliver_scheduled_queue_at(TASK, DUE);
    f.settled();
    f.api.deliver_scheduled_queue_at(TASK, DUE);
    assert_eq!(f.agent.prompts.load(Ordering::SeqCst), 1);
    assert_eq!(
        f.agent.prompt_calls.lock().unwrap()[0].1,
        "scheduled instruction"
    );
    assert!(f
        .store
        .read_task(TASK)
        .unwrap()
        .message_queue
        .items
        .is_empty());
}

#[test]
fn scheduled_queue_waits_for_active_work_and_normal_settlement_does_not_send_early() {
    let f = Fixture::new(true);
    f.api.send(send_params(TASK, "current turn")).unwrap();
    wait_until(|| f.store.read_task(TASK).unwrap().status == TaskStatus::Active);
    f.schedule("future turn");
    f.api.deliver_scheduled_queue_at(TASK, DUE);
    assert_eq!(f.agent.prompts.load(Ordering::SeqCst), 1);
    assert_eq!(f.agent.steers.load(Ordering::SeqCst), 0);
    f.agent.release_prompt.store(true, Ordering::SeqCst);
    f.settled();
    assert_eq!(f.agent.prompts.load(Ordering::SeqCst), 1);
    assert_eq!(
        f.store.read_task(TASK).unwrap().message_queue.items.len(),
        1
    );
    f.api.deliver_scheduled_queue_at(TASK, DUE);
    f.settled();
    assert_eq!(f.agent.prompts.load(Ordering::SeqCst), 2);
}

#[test]
fn scheduled_queue_restart_requires_resume_and_resume_preserves_future_time() {
    let f = Fixture::new(false);
    f.schedule("after restart");
    let recovered = listing_activity_api(&f.store);
    let task = f.store.read_task(TASK).unwrap();
    assert_eq!(
        task.message_queue.pause,
        Some(TaskMessageQueuePauseRecord::Restarted)
    );
    assert_eq!(task.message_queue.items[0].not_before.as_deref(), Some(DUE));
    recovered.deliver_scheduled_queue_at(TASK, DUE);
    assert_eq!(
        f.store.read_task(TASK).unwrap().message_queue.items.len(),
        1
    );
    let client = crate::attachment_runtime::AttachmentOwner::test_client_instance_id();
    assert!(recovered
        .queue_resume_for_client(
            &client,
            TaskQueueResumeParams {
                task_id: TASK.into(),
                queue_revision: task.message_queue.revision - 1,
            }
        )
        .is_err());
    recovered
        .queue_resume_for_client(
            &client,
            TaskQueueResumeParams {
                task_id: TASK.into(),
                queue_revision: task.message_queue.revision,
            },
        )
        .unwrap();
    recovered.deliver_scheduled_queue_at(TASK, BEFORE);
    assert_eq!(
        f.store.read_task(TASK).unwrap().message_queue.items.len(),
        1
    );
    recovered.deliver_scheduled_queue_at(TASK, DUE);
    f.settled();
    assert!(f
        .store
        .read_task(TASK)
        .unwrap()
        .message_queue
        .items
        .is_empty());
}

#[test]
fn scheduled_queue_manual_send_overrides_time_and_removal_cancels_delivery() {
    let f = Fixture::new(false);
    let queued = f.schedule("send immediately");
    f.api
        .send(TaskSendParams {
            task_id: TASK.into(),
            message: ComposerMessage {
                text: Some("send immediately".into()),
                ..Default::default()
            },
            queue_selection: Some(TaskQueueSendSelection {
                queued_message_id: queued.message_queue.items[0].queued_message_id.clone(),
                queue_revision: queued.message_queue.revision,
            }),
        })
        .unwrap();
    f.settled();
    let queued = f.schedule("cancel this");
    f.api
        .queue_remove_for_test(TaskQueueRemoveParams {
            task_id: TASK.into(),
            queued_message_id: queued.message_queue.items[0].queued_message_id.clone(),
            queue_revision: queued.message_queue.revision,
            client_mutation_id: "remove-scheduled".into(),
        })
        .unwrap();
    f.api.deliver_scheduled_queue_at(TASK, DUE);
    assert_eq!(f.agent.prompts.load(Ordering::SeqCst), 1);
}

#[test]
fn scheduled_queue_rejects_invalid_times_without_accepting_content() {
    let f = Fixture::new(false);
    for value in ["", "tomorrow", "0", "-1", "999999999999999999999"] {
        assert!(f
            .api
            .queue_append_for_test(TaskQueueAppendParams {
                task_id: TASK.into(),
                message: ComposerMessage {
                    text: Some("not accepted".into()),
                    ..Default::default()
                },
                not_before: Some(value.into()),
            })
            .is_err());
    }
    assert!(f
        .store
        .read_task(TASK)
        .unwrap()
        .message_queue
        .items
        .is_empty());
}

#[test]
fn scheduled_queue_poll_uses_registered_work_and_coalesces_wakes() {
    let f = Fixture::new(false);
    f.schedule("timer delivery");
    // Move the persisted deadline into the past instead of relying on wall-clock sleeps.
    f.api
        .mutations
        .commit_existing_task(TASK, super::super::response_snapshot_options(), |ctx| {
            ctx.task_mut().message_queue.items[0].not_before = Some("1".into());
            Ok(TaskMutationResult::Changed)
        })
        .unwrap();
    f.api.poll_scheduled_queue();
    f.api.poll_scheduled_queue();
    wait_until(|| f.agent.prompts.load(Ordering::SeqCst) == 1);
    f.settled();
    assert!(f
        .store
        .read_task(TASK)
        .unwrap()
        .message_queue
        .items
        .is_empty());
}

#[test]
fn scheduled_queue_future_head_blocks_an_earlier_tail_until_reordered() {
    let f = Fixture::new(false);
    let first = f.schedule("first");
    let second = f
        .api
        .queue_append_for_test(TaskQueueAppendParams {
            task_id: TASK.into(),
            not_before: Some(BEFORE.into()),
            message: ComposerMessage {
                text: Some("second".into()),
                ..Default::default()
            },
        })
        .unwrap();
    f.api.deliver_scheduled_queue_at(TASK, BEFORE);
    assert_eq!(f.agent.prompts.load(Ordering::SeqCst), 0);
    f.api
        .queue_move_for_test(TaskQueueMoveParams {
            task_id: TASK.into(),
            queued_message_id: second.message_queue.items[1].queued_message_id.clone(),
            queue_revision: second.message_queue.revision,
            target_index: 0,
            client_mutation_id: "reorder".into(),
        })
        .unwrap();
    f.api.deliver_scheduled_queue_at(TASK, BEFORE);
    f.settled();
    assert_eq!(f.agent.prompt_calls.lock().unwrap()[0].1, "second");
    assert_eq!(
        f.store.read_task(TASK).unwrap().message_queue.items[0].queued_message_id,
        first.message_queue.items[0].queued_message_id.as_str()
    );
}

#[test]
fn scheduled_queue_delivery_failure_preserves_and_pauses_the_item() {
    let f = Fixture::new(false);
    f.schedule("workspace unavailable");
    f.api
        .mutations
        .commit_existing_task(TASK, super::super::response_snapshot_options(), |ctx| {
            ctx.task_mut().workspace_root = f._root.path().join("missing").to_string_lossy().into();
            Ok(TaskMutationResult::Changed)
        })
        .unwrap();
    f.api.deliver_scheduled_queue_at(TASK, DUE);
    let task = f.store.read_task(TASK).unwrap();
    assert_eq!(task.message_queue.items.len(), 1);
    assert_eq!(
        task.message_queue.pause,
        Some(TaskMessageQueuePauseRecord::UnsuccessfulTurn)
    );
    f.api.deliver_scheduled_queue_at(TASK, DUE);
    assert_eq!(f.agent.prompts.load(Ordering::SeqCst), 0);
}

#[test]
fn scheduled_queue_missing_attachment_preserves_item_with_attachment_pause() {
    let f = Fixture::new(false);
    let file = f._root.path().join("scheduled-attachment.txt");
    std::fs::write(&file, "fixture attachment").unwrap();
    let attachment = f.api.attachment_runtime().register_file_reference_for_test(
        TaskId::from(TASK),
        "scheduled-attachment.txt",
        &file,
    );
    let accepted = f
        .api
        .queue_append_for_test(TaskQueueAppendParams {
            task_id: TASK.into(),
            message: ComposerMessage {
                text: Some("scheduled with attachment".into()),
                attachments: vec![attachment.handle_id],
                ..Default::default()
            },
            not_before: Some(DUE.into()),
        })
        .unwrap();
    std::fs::remove_file(&file).unwrap();

    f.api.deliver_scheduled_queue_at(TASK, DUE);

    let task = f.store.read_task(TASK).unwrap();
    assert_eq!(task.message_queue.items.len(), 1);
    assert_eq!(
        task.message_queue.items[0].queued_message_id,
        accepted.message_queue.items[0].queued_message_id.as_str()
    );
    assert_eq!(
        task.message_queue.pause,
        Some(TaskMessageQueuePauseRecord::AttachmentUnavailable)
    );
    assert_eq!(f.agent.prompts.load(Ordering::SeqCst), 0);
    assert!(task.active_turn_id.is_none());
}

#[test]
fn scheduled_queue_logs_its_wake_before_waiting_for_acceptance() {
    let f = Fixture::new(false);
    const LOG_TASK: &str = "scheduled-log-task";
    f.store
        .write_task(&task_record(
            LOG_TASK,
            &f._root.path().join("workspace").to_string_lossy(),
        ))
        .unwrap();
    f.api
        .queue_append_for_test(TaskQueueAppendParams {
            task_id: LOG_TASK.into(),
            not_before: Some(DUE.into()),
            message: ComposerMessage {
                text: Some("private scheduled text".into()),
                ..Default::default()
            },
        })
        .unwrap();
    let capture = crate::logging::capture_test_logs();
    f.api.deliver_scheduled_queue_at(LOG_TASK, BEFORE);
    assert!(!capture
        .snapshot()
        .iter()
        .any(|event| event["event"] == "task_scheduled_wake_started"
            && event["fields"]["task_id"] == LOG_TASK));

    let (held, acquired) = std::sync::mpsc::channel();
    let (release, released) = std::sync::mpsc::channel();
    let gate = f.api.turn_acceptance.clone();
    let holder = std::thread::spawn(move || {
        gate.serialize(LOG_TASK, || {
            held.send(()).unwrap();
            released.recv().unwrap();
        })
    });
    acquired.recv().unwrap();
    let api = f.api.clone();
    let delivery = std::thread::spawn(move || api.deliver_scheduled_queue_at(LOG_TASK, DUE));
    let waiting = std::cell::RefCell::new(Vec::new());
    wait_until(|| {
        let mut waiting = waiting.borrow_mut();
        waiting.extend(capture.snapshot());
        waiting.iter().any(|event| {
            event["event"] == "task_scheduled_wake_started"
                && event["fields"]["task_id"] == LOG_TASK
        })
    });
    let mut waiting = waiting.into_inner();
    assert!(!waiting
        .iter()
        .any(|event| event["event"] == "task_scheduled_wake_completed"
            && event["fields"]["task_id"] == LOG_TASK));
    release.send(()).unwrap();
    holder.join().unwrap();
    delivery.join().unwrap();
    wait_until(|| f.store.read_task(LOG_TASK).unwrap().status == TaskStatus::Inactive);
    waiting.extend(capture.snapshot());
    let start = waiting
        .iter()
        .find(|event| {
            event["event"] == "task_scheduled_wake_started"
                && event["fields"]["task_id"] == LOG_TASK
        })
        .unwrap();
    let operation = &start["fields"]["operation_id"];
    assert!(operation.as_str().is_some_and(|value| !value.is_empty()));
    let events: Vec<_> = waiting
        .iter()
        .filter(|event| &event["fields"]["operation_id"] == operation)
        .collect();
    assert!(events
        .iter()
        .any(|event| event["event"] == "task_scheduled_wake_admitted"
            && event["fields"]["wait_duration_ms"].is_number()));
    assert!(events
        .iter()
        .any(|event| event["event"] == "task_scheduled_wake_completed"
            && event["fields"]["outcome"] == "accepted"));
    assert!(!serde_json::to_string(&events)
        .unwrap()
        .contains("private scheduled text"));
}
