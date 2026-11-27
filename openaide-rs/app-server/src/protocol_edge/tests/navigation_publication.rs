//! Navigation frames at the protocol edge: one owner, flushed under the protocol lock, with
//! frames queued ahead of the reply of the request that caused them.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::events::AppServerEventPayload;
use openaide_app_server_protocol::ids::ProjectId;
use openaide_app_server_protocol::methods::{
    NAVIGATION_SUBSCRIBE, NAVIGATION_UNSUBSCRIBE, NAVIGATION_UPDATE,
};
use openaide_app_server_protocol::navigation::{NavigationSubscribeResult, NavigationViewParams};
use openaide_app_server_protocol::snapshot::{
    TaskLifecycle, TaskNavigationEntry, TaskStatus, TaskSummary,
};
use openaide_app_server_protocol::task::TaskNavigationSection;

use crate::native_sessions::discovery_state::{DiscoveryState, ProjectDiscovery};
use crate::navigation::{NavigationDirty, NavigationOwner, NavigationSource, SourceGroup};

use super::*;
use crate::protocol_edge::outbox::OutboundMessage;

/// One Project whose Task titles the test changes, and which records whether the protocol lock
/// was held at each read.
#[derive(Default)]
struct LockObservingSource {
    revision: AtomicU64,
    gateway: OnceLock<SharedRpcGateway>,
    reads: AtomicU64,
    reads_while_locked: AtomicU64,
    log: Mutex<Vec<u64>>,
}

impl NavigationSource for LockObservingSource {
    fn groups(
        &self,
        _section: TaskNavigationSection,
        _projects: Option<&HashSet<ProjectId>>,
    ) -> Result<Vec<SourceGroup>, ProtocolError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        if let Some(gateway) = self.gateway.get() {
            if gateway.try_lock_for_test().is_none() {
                self.reads_while_locked.fetch_add(1, Ordering::SeqCst);
            }
        }
        let revision = self.revision.load(Ordering::SeqCst);
        self.log.lock().unwrap().push(revision);
        Ok(vec![SourceGroup {
            project_id: "project-1".into(),
            label: "Project".to_string(),
            entries: vec![TaskNavigationEntry::Task {
                task: Box::new(TaskSummary {
                    task_id: "task-1".into(),
                    project_id: "project-1".into(),
                    agent_id: "agent".into(),
                    lifecycle: TaskLifecycle::Open,
                    title: None,
                    status: TaskStatus::Idle,
                    updated_at: revision.to_string(),
                    last_activity: revision.to_string(),
                    unread: false,
                    pinned: false,
                    attention: None,
                    has_messages: true,
                    worktree_id: None,
                    workspace_available: true,
                }),
            }],
        }])
    }

    fn discovery(&self, _project_id: &ProjectId) -> ProjectDiscovery {
        ProjectDiscovery::default()
    }
}

struct Setup {
    gateway: SharedRpcGateway,
    connection_id: ConnectionId,
    source: Arc<LockObservingSource>,
    dirty: NavigationDirty,
}

fn setup() -> Setup {
    let source = Arc::new(LockObservingSource::default());
    let dirty = NavigationDirty::default();
    let owner = NavigationOwner::new(source.clone(), dirty.clone(), DiscoveryState::default());
    let shared = SharedRpcGateway::new(gateway().with_navigation(owner));
    assert!(source.gateway.set(shared.clone()).is_ok());
    let connection_id = ConnectionId::new("conn-navigation");
    shared.handle_inbound(
        connection_id.clone(),
        request("1", CLIENT_INITIALIZE, init_params("client-navigation")),
        AppServerTime(1),
    );
    Setup {
        gateway: shared,
        connection_id,
        source,
        dirty,
    }
}

fn view(limit: u32) -> NavigationViewParams {
    NavigationViewParams {
        section: TaskNavigationSection::Tasks,
        project_ids: None,
        history_limit: limit,
        history_limits: Vec::new(),
        open_row: None,
    }
}

fn subscribe_view(setup: &Setup) -> NavigationSubscribeResult {
    let outcome = setup.gateway.handle_inbound(
        setup.connection_id.clone(),
        request("sub", NAVIGATION_SUBSCRIBE, view(7)),
        AppServerTime(2),
    );
    let GatewayOutcome::Respond { response, .. } = outcome else {
        panic!("subscribe was not answered");
    };
    serde_json::from_value(response_result(&response)).unwrap()
}

fn response_result(response: &GatewayResponse) -> serde_json::Value {
    match response {
        GatewayResponse::Result(result) => result["result"].clone(),
        other => panic!("not a result: {other:?}"),
    }
}

/// Everything queued for the connection, as `reply:<id>` or the frame's first row order.
fn drain_order(setup: &Setup) -> Vec<String> {
    setup
        .gateway
        .transfer_outbound(&setup.connection_id, |messages| {
            messages
                .into_iter()
                .map(|message| match message {
                    OutboundMessage::Reply { id, .. } => format!("reply:{}", id.as_str().unwrap()),
                    OutboundMessage::Event(event) => match event.payload {
                        AppServerEventPayload::NavigationFrame { frame } => {
                            format!("frame:{}", frame.groups[0].rows.len())
                        }
                        _ => "event".to_string(),
                    },
                    OutboundMessage::ServerRequest(_) => "server-request".to_string(),
                    OutboundMessage::Sync(_) => "sync".to_string(),
                })
                .collect()
        })
}

#[test]
fn subscribe_answers_with_the_baseline_and_a_cursor_that_links_the_next_frame() {
    let setup = setup();

    let subscribed = subscribe_view(&setup);
    drain_order(&setup);
    setup.source.revision.store(5, Ordering::SeqCst);
    setup.dirty.mark_project("project-1".into());
    setup
        .gateway
        .publish_committed_task_update(&wake(), AppServerTime(3));

    assert_eq!(subscribed.frame.groups[0].order, vec!["task:task-1"]);
    assert_eq!(subscribed.frame.group_order, Some(vec!["project-1".into()]));
    let events = setup
        .gateway
        .drain_event_deliveries_for_connection(&setup.connection_id);
    assert_eq!(events.len(), 1, "{events:?}");
    // The frame orders after the baseline the client holds.
    let AppServerEventPayload::NavigationFrame { frame } = &events[0].payload else {
        panic!("expected a Navigation frame");
    };
    assert!(frame.revision > subscribed.frame.revision);
}

#[test]
fn a_mutation_marks_dirty_and_its_frame_is_queued_before_its_reply() {
    let setup = setup();
    subscribe_view(&setup);
    drain_order(&setup);
    setup.source.revision.store(9, Ordering::SeqCst);
    // The mutation's commit marks the Project; the request that follows carries the reply.
    setup.dirty.mark_project("project-1".into());

    setup.gateway.dispatch_inbound(
        setup.connection_id.clone(),
        request(
            "next",
            NAVIGATION_UPDATE,
            NavigationViewParams {
                history_limit: 8,
                ..view(7)
            },
        ),
        AppServerTime(3),
    );

    let order = drain_order(&setup);
    assert_eq!(order, vec!["frame:1", "reply:next"], "{order:?}");
}

#[test]
fn every_recompute_reads_under_the_protocol_lock() {
    // A read before the lock could be overtaken by a mutation that publishes under it, which
    // is what the old generation counter patched. The owner has no such window.
    let setup = setup();
    subscribe_view(&setup);
    for revision in 1..=4 {
        setup.source.revision.store(revision, Ordering::SeqCst);
        setup.dirty.mark_project("project-1".into());
        setup
            .gateway
            .publish_committed_task_update(&wake(), AppServerTime(3));
    }

    assert!(setup.source.reads.load(Ordering::SeqCst) >= 5);
    assert_eq!(
        setup.source.reads_while_locked.load(Ordering::SeqCst),
        setup.source.reads.load(Ordering::SeqCst),
        "every read holds the protocol lock"
    );
    let log = setup.source.log.lock().unwrap().clone();
    assert!(
        log.windows(2).all(|pair| pair[0] <= pair[1]),
        "a recompute moved clients backwards: {log:?}"
    );
}

#[test]
fn unsubscribe_stops_frames() {
    let setup = setup();
    subscribe_view(&setup);
    setup.gateway.handle_inbound(
        setup.connection_id.clone(),
        request(
            "unsub",
            NAVIGATION_UNSUBSCRIBE,
            openaide_app_server_protocol::navigation::NavigationUnsubscribeParams {
                section: TaskNavigationSection::Tasks,
            },
        ),
        AppServerTime(3),
    );
    drain_order(&setup);
    setup.source.revision.store(3, Ordering::SeqCst);
    setup.dirty.mark_project("project-1".into());

    setup
        .gateway
        .publish_committed_task_update(&wake(), AppServerTime(4));

    assert!(drain_order(&setup).is_empty());
}

fn wake() -> TaskUpdate {
    TaskUpdate {
        task_id: String::new(),
        revision: 0,
        kind: TaskUpdateKind::NavigationDirty,
    }
}

#[test]
fn raising_a_history_limit_asks_discovery_to_deepen_only_the_shown_project() {
    let workflow = Arc::new(RecordingCatalogRefresh::default());
    let source = Arc::new(LockObservingSource::default());
    let owner = NavigationOwner::new(
        source,
        NavigationDirty::default(),
        DiscoveryState::default(),
    );
    let shared = SharedRpcGateway::new(
        gateway_with_agent_session_listing(workflow.clone()).with_navigation(owner),
    );
    let connection_id = ConnectionId::new("conn-deepen");
    shared.handle_inbound(
        connection_id.clone(),
        request("1", CLIENT_INITIALIZE, init_params("client-deepen")),
        AppServerTime(1),
    );

    shared.handle_inbound(
        connection_id.clone(),
        request("2", NAVIGATION_SUBSCRIBE, view(7)),
        AppServerTime(2),
    );
    assert_eq!(*workflow.deepening.lock().unwrap(), ["project-1"]);

    // A lower or equal limit asks for nothing; a higher one asks again.
    shared.handle_inbound(
        connection_id.clone(),
        request("3", NAVIGATION_UPDATE, view(3)),
        AppServerTime(3),
    );
    assert_eq!(workflow.deepening.lock().unwrap().len(), 1);
    shared.handle_inbound(
        connection_id,
        request("4", NAVIGATION_UPDATE, view(30)),
        AppServerTime(4),
    );
    assert_eq!(workflow.deepening.lock().unwrap().len(), 2);
}

/// Many history Tasks in one Project; discovery reports `loading` once deepening was asked for.
#[derive(Default)]
struct DeepeningSource {
    loading: std::sync::atomic::AtomicBool,
}

impl NavigationSource for DeepeningSource {
    fn groups(
        &self,
        _section: TaskNavigationSection,
        _projects: Option<&HashSet<ProjectId>>,
    ) -> Result<Vec<SourceGroup>, ProtocolError> {
        let entries = (0..6)
            .map(|index| TaskNavigationEntry::Task {
                task: Box::new(TaskSummary {
                    task_id: format!("task-{index}").into(),
                    project_id: "project-1".into(),
                    agent_id: "agent".into(),
                    lifecycle: TaskLifecycle::Open,
                    title: None,
                    status: TaskStatus::Idle,
                    updated_at: format!("2026-01-0{}T00:00:00Z", index + 1),
                    last_activity: format!("2026-01-0{}T00:00:00Z", index + 1),
                    unread: false,
                    pinned: false,
                    attention: None,
                    has_messages: true,
                    worktree_id: None,
                    workspace_available: true,
                }),
            })
            .collect();
        Ok(vec![SourceGroup {
            project_id: "project-1".into(),
            label: "Project".to_string(),
            entries,
        }])
    }

    fn discovery(&self, _project_id: &ProjectId) -> ProjectDiscovery {
        ProjectDiscovery {
            loading: self.loading.load(Ordering::SeqCst),
            ..Default::default()
        }
    }
}

/// Asking for deeper discovery starts it, which marks Navigation dirty during the request.
struct DiscoveryStartsOnDeepening {
    source: Arc<DeepeningSource>,
    dirty: NavigationDirty,
}

impl AgentListSessionsWorkflow for DiscoveryStartsOnDeepening {
    fn list_agent_sessions(
        &self,
        _params: openaide_app_server_protocol::agent::AgentListSessionsParams,
    ) -> Result<openaide_app_server_protocol::agent::AgentListSessionsResult, ProtocolError> {
        Err(test_unavailable("interactive listing is not used"))
    }

    fn request_native_session_discovery_deepening(&self, _project_id: &str) {
        self.source.loading.store(true, Ordering::SeqCst);
        self.dirty.mark_project("project-1".into());
    }
}

#[test]
fn frames_one_request_produces_reach_the_client_in_revision_order() {
    // The update's own frame reveals rows (bodies); deepening then flips discovery to loading,
    // which the flush before the reply publishes as a delta without those bodies. If the delta
    // were queued first, the client would see rows it never received and lose the view.
    let source = Arc::new(DeepeningSource::default());
    let dirty = NavigationDirty::default();
    let owner = NavigationOwner::new(source.clone(), dirty.clone(), DiscoveryState::default());
    let workflow = Arc::new(DiscoveryStartsOnDeepening {
        source: source.clone(),
        dirty,
    });
    let shared =
        SharedRpcGateway::new(gateway_with_agent_session_listing(workflow).with_navigation(owner));
    let connection_id = ConnectionId::new("conn-order");
    shared.handle_inbound(
        connection_id.clone(),
        request("1", CLIENT_INITIALIZE, init_params("client-order")),
        AppServerTime(1),
    );
    shared.handle_inbound(
        connection_id.clone(),
        request("sub", NAVIGATION_SUBSCRIBE, view(2)),
        AppServerTime(2),
    );
    shared.transfer_outbound(&connection_id, |_| ());
    // The first discovery pass finished; the update's deeper target starts another.
    source.loading.store(false, Ordering::SeqCst);

    let logs = crate::logging::capture_test_logs();
    shared.dispatch_inbound(
        connection_id.clone(),
        request("up", NAVIGATION_UPDATE, view(5)),
        AppServerTime(3),
    );
    let published: Vec<_> = logs
        .snapshot()
        .into_iter()
        .filter(|line| line["event"] == "navigation_frame_published")
        .filter(|line| line["fields"]["client_instance_id"] == "client-order")
        .collect();
    assert_eq!(published.len(), 2, "{published:?}");
    assert_eq!(published[0]["level"], "debug");
    assert_eq!(published[0]["fields"]["section"], "Tasks");
    assert_eq!(published[0]["fields"]["group_count"], 1);
    assert_eq!(published[0]["fields"]["row_body_count"], 3);
    assert_eq!(published[0]["fields"]["order_len"], 5);
    assert_eq!(published[0]["fields"]["ahead_of_reply"], true);
    assert_eq!(published[1]["fields"]["row_body_count"], 0);
    assert!(
        published[0]["fields"]["revision"].as_u64() < published[1]["fields"]["revision"].as_u64()
    );

    let (revisions, bodies) = shared.transfer_outbound(&connection_id, |messages| {
        let frames: Vec<_> = messages
            .iter()
            .filter_map(|message| match message {
                OutboundMessage::Event(event) => match &event.payload {
                    AppServerEventPayload::NavigationFrame { frame } => Some(frame.clone()),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        let last = messages.last().expect("the reply is queued last");
        assert!(matches!(last, OutboundMessage::Reply { .. }));
        (
            frames
                .iter()
                .map(|frame| frame.revision)
                .collect::<Vec<_>>(),
            frames
                .iter()
                .map(|frame| frame.groups[0].rows.len())
                .collect::<Vec<_>>(),
        )
    });
    assert_eq!(bodies, vec![3, 0], "revealed rows first, then the delta");
    assert!(revisions[0] < revisions[1], "{revisions:?}");
}
