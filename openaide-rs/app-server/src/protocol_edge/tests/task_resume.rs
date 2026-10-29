//! `state/subscribe` with a known Task revision (ADR-0073).

use super::*;

fn open_task(task_id: &str, revision: u64) -> crate::storage::records::TaskRecord {
    let mut task = client_new_task_record(task_id, "client-1");
    task.lifecycle = crate::storage::records::TaskLifecycle::Open;
    task.revision = revision;
    task
}

fn subscribe(
    gateway: &mut RpcGateway,
    task_id: &str,
    known_revision: Option<u64>,
) -> GatewayOutcome {
    gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        request(
            "subscribe",
            STATE_SUBSCRIBE,
            StateSubscribeParams {
                scope: SubscriptionScope::Task {
                    task_id: TaskId::from(task_id),
                },
                known_revision,
            },
        ),
        AppServerTime(10),
    )
}

fn result_of(outcome: GatewayOutcome) -> serde_json::Value {
    response_value(outcome)["result"].clone()
}

fn unsubscribe(gateway: &mut RpcGateway, task_id: &str) {
    response_value(gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        request(
            "unsubscribe",
            STATE_UNSUBSCRIBE,
            StateUnsubscribeParams {
                scope: SubscriptionScope::Task {
                    task_id: TaskId::from(task_id),
                },
            },
        ),
        AppServerTime(11),
    ));
}

fn publish_revisions(
    gateway: &mut RpcGateway,
    task_id: &str,
    revisions: std::ops::RangeInclusive<u64>,
) {
    for revision in revisions {
        gateway.publish_task_update(
            &committed_task_update(
                task_id,
                revision,
                Vec::new(),
                Vec::new(),
                TestNavigationChange::None,
            ),
            AppServerTime(revision),
        );
    }
}

/// A Task the client saw at revision 1, which then advanced to 3 while unsubscribed.
fn gateway_after_missed_revisions(task_id: &str) -> RpcGateway {
    let (mut gateway, store) = gateway_with_project_context_and_store();
    store.write_task(&open_task(task_id, 1)).unwrap();
    initialize_client(&mut gateway, ConnectionId::new("conn-1"));
    let baseline = result_of(subscribe(&mut gateway, task_id, None));
    assert_eq!(baseline["snapshot"]["kind"], json!("task"));
    unsubscribe(&mut gateway, task_id);
    publish_revisions(&mut gateway, task_id, 2..=3);
    store.write_task(&open_task(task_id, 3)).unwrap();
    gateway
}

/// Tests share the process log sink, so each test reads only its own Task's events.
fn resolutions(logs: &crate::logging::TestLogCapture, task_id: &str) -> Vec<serde_json::Value> {
    logs.snapshot()
        .into_iter()
        .filter(|entry| entry["event"] == json!("task_subscription_resolved"))
        .filter(|entry| entry["fields"]["task_id"] == json!(task_id))
        .map(|entry| entry["fields"].clone())
        .collect()
}

#[test]
fn a_known_revision_receives_only_the_missed_changes() {
    let task_id = "task-resume-missed";
    let mut gateway = gateway_after_missed_revisions(task_id);
    let logs = crate::logging::capture_test_logs();

    let result = result_of(subscribe(&mut gateway, task_id, Some(1)));

    assert_eq!(result["snapshot"]["kind"], json!("taskResume"));
    let resume = &result["snapshot"]["resume"];
    assert_eq!(resume["knownRevision"], json!(1));
    let revisions = resume["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|change| change["revision"].clone())
        .collect::<Vec<_>>();
    assert_eq!(revisions, vec![json!(2), json!(3)]);
    assert_eq!(resume["pendingRequests"], json!([]));
    assert_eq!(resume["workspaceAvailable"], json!(false));
    assert!(result["cursor"].is_string());
    let [fields] = resolutions(&logs, task_id).try_into().unwrap();
    assert_eq!(fields["outcome"], json!("missedChanges"));
    assert_eq!(fields["known_revision"], json!(1));
    assert_eq!(fields["current_revision"], json!(3));
    assert_eq!(fields["change_count"], json!(2));
    assert!(fields["change_bytes"].as_u64().unwrap() > 0);
}

#[test]
fn a_current_revision_receives_no_task_state() {
    let task_id = "task-resume-current";
    let mut gateway = gateway_after_missed_revisions(task_id);
    let logs = crate::logging::capture_test_logs();

    let result = result_of(subscribe(&mut gateway, task_id, Some(3)));

    assert_eq!(result["snapshot"]["kind"], json!("taskResume"));
    assert_eq!(result["snapshot"]["resume"]["changes"], json!([]));
    let [fields] = resolutions(&logs, task_id).try_into().unwrap();
    assert_eq!(fields["outcome"], json!("current"));
}

#[test]
fn an_unusable_known_revision_receives_a_baseline() {
    for (known_revision, reason) in [(5, "knownRevisionAhead"), (0, "changesEvicted")] {
        let task_id = format!("task-resume-{reason}");
        let mut gateway = gateway_after_missed_revisions(&task_id);
        let logs = crate::logging::capture_test_logs();

        let result = result_of(subscribe(&mut gateway, &task_id, Some(known_revision)));

        assert_eq!(result["snapshot"]["kind"], json!("task"), "{reason}");
        assert_eq!(result["snapshot"]["task"]["revision"], json!(3), "{reason}");
        let [fields] = resolutions(&logs, &task_id).try_into().unwrap();
        assert_eq!(fields["outcome"], json!("baseline"));
        assert_eq!(fields["baseline_reason"], json!(reason));
    }
}

#[test]
fn changes_this_process_never_published_receive_a_baseline() {
    let task_id = "task-resume-unpublished";
    let (mut gateway, store) = gateway_with_project_context_and_store();
    store.write_task(&open_task(task_id, 4)).unwrap();
    initialize_client(&mut gateway, ConnectionId::new("conn-1"));
    let logs = crate::logging::capture_test_logs();

    let result = result_of(subscribe(&mut gateway, task_id, Some(2)));

    assert_eq!(result["snapshot"]["kind"], json!("task"));
    let [fields] = resolutions(&logs, task_id).try_into().unwrap();
    assert_eq!(fields["baseline_reason"], json!("changesNotRetained"));
}

#[test]
fn another_clients_prepared_task_is_not_found_with_a_known_revision() {
    let task_id = "task-resume-prepared";
    let (mut gateway, store) = gateway_with_project_context_and_store();
    store
        .write_task(&client_new_task_record(task_id, "client-2"))
        .unwrap();
    initialize_client(&mut gateway, ConnectionId::new("conn-1"));

    let error = response_error(subscribe(&mut gateway, task_id, Some(1)));

    assert_eq!(error.error.code, ProtocolErrorCode::NotFound);
}
