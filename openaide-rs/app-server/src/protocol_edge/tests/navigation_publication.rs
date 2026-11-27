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
    assert_eq!(events[0].previous_cursor, subscribed.cursor);
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
