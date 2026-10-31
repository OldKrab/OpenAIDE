use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::OnceLock;

use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::events::{AppServerEventPayload, TaskChanges};
use openaide_app_server_protocol::ids::ProjectId;
use openaide_app_server_protocol::snapshot::{
    TaskNavigationGroup, TaskNavigationRefreshState, TaskNavigationSnapshot,
};
use openaide_app_server_protocol::task::TaskNavigationSection;

use crate::snapshots::TaskNavigationSnapshotSource;
use crate::task_events::{
    CommittedNavigationChange, CommittedTaskChange, TaskUpdate, TaskUpdateKind,
};

use super::*;

/// Reports each read's number as the Task count, so a test can tell which read was published.
#[derive(Default)]
struct CountingNavigation {
    reads: AtomicU64,
    gateway: OnceLock<SharedRpcGateway>,
    /// Task count of the read that overlaps a direct publication.
    overtaken_read: AtomicU64,
    overtake_with_full_replacement: AtomicBool,
    read_while_locked: AtomicU64,
}

impl TaskNavigationSnapshotSource for CountingNavigation {
    fn snapshot(
        &self,
        section: TaskNavigationSection,
        project_ids: Option<&[ProjectId]>,
    ) -> Result<TaskNavigationSnapshot, ProtocolError> {
        let read = self.reads.fetch_add(1, Ordering::SeqCst) + 1;
        if let Some(gateway) = self.gateway.get() {
            match gateway.try_lock_for_test() {
                Some(mut locked) => {
                    if self.overtaken_read.load(Ordering::SeqCst) == read {
                        if self.overtake_with_full_replacement.load(Ordering::SeqCst) {
                            locked.publish_navigation_replacement(AppServerTime(4));
                        } else {
                            locked.publish_project_entries_replaced(
                                &ProjectId::from("project-1"),
                                AppServerTime(4),
                            );
                        }
                    }
                }
                None => {
                    self.read_while_locked.fetch_add(1, Ordering::SeqCst);
                }
            }
        }
        Ok(TaskNavigationSnapshot {
            section,
            groups: project_ids
                .unwrap_or_default()
                .iter()
                .map(|project_id| TaskNavigationGroup {
                    project_id: project_id.clone(),
                    project_label: "Project".into(),
                    task_count: read,
                    entries: Vec::new(),
                    has_more: false,
                    loading: false,
                })
                .collect(),
            refresh: TaskNavigationRefreshState::default(),
        })
    }
}

fn navigation_gateway(navigation: Arc<CountingNavigation>) -> (SharedRpcGateway, ConnectionId) {
    let mut gateway = gateway();
    gateway.snapshots = SnapshotBuilder::with_task_navigation(
        "server-1".into(),
        "root-1".into(),
        navigation.clone(),
    );
    let shared = SharedRpcGateway::new(gateway);
    let connection_id = ConnectionId::new("conn-navigation-publication");
    shared.handle_inbound(
        connection_id.clone(),
        request(
            "1",
            CLIENT_INITIALIZE,
            init_params("client-navigation-publication"),
        ),
        AppServerTime(1),
    );
    shared.handle_inbound(
        connection_id.clone(),
        request(
            "2",
            STATE_SUBSCRIBE,
            StateSubscribeParams {
                scope: SubscriptionScope::TaskNavigation {
                    section: TaskNavigationSection::Tasks,
                    project_ids: None,
                },
                known_revision: None,
            },
        ),
        AppServerTime(2),
    );
    shared.drain_event_deliveries_for_connection(&connection_id);
    assert!(navigation.gateway.set(shared.clone()).is_ok());
    (shared, connection_id)
}

fn entries_changed() -> TaskUpdate {
    TaskUpdate {
        task_id: "task-1".into(),
        revision: 1,
        kind: TaskUpdateKind::NavigationProjectEntriesChanged {
            project_id: ProjectId::from("project-1"),
        },
    }
}

fn published_task_counts(gateway: &SharedRpcGateway, connection_id: &ConnectionId) -> Vec<u64> {
    gateway
        .drain_event_deliveries_for_connection(connection_id)
        .into_iter()
        .filter_map(|delivery| match delivery.event.payload {
            AppServerEventPayload::ProjectEntriesReplaced {
                section: TaskNavigationSection::Tasks,
                task_count,
                ..
            } => Some(task_count),
            _ => None,
        })
        .collect()
}

#[test]
fn a_navigation_publication_reads_without_the_protocol_lock() {
    let navigation = Arc::new(CountingNavigation::default());
    let (gateway, connection_id) = navigation_gateway(navigation.clone());

    gateway.publish_committed_task_update(&entries_changed(), AppServerTime(3));

    assert_eq!(navigation.read_while_locked.load(Ordering::SeqCst), 0);
    assert_eq!(published_task_counts(&gateway, &connection_id).len(), 1);
}

#[test]
fn a_committed_change_reads_its_navigation_without_the_protocol_lock() {
    let navigation = Arc::new(CountingNavigation::default());
    let (gateway, connection_id) = navigation_gateway(navigation.clone());
    let change = CommittedTaskChange {
        changes: TaskChanges::default(),
        tool_details: Vec::new(),
        navigation: Some(CommittedNavigationChange::ProjectEntriesChanged {
            project_id: ProjectId::from("project-1"),
        }),
    };

    gateway.publish_committed_task_update(
        &TaskUpdate {
            task_id: "task-1".into(),
            revision: 1,
            kind: TaskUpdateKind::Changed(Box::new(change)),
        },
        AppServerTime(3),
    );

    assert_eq!(navigation.read_while_locked.load(Ordering::SeqCst), 0);
    assert_eq!(published_task_counts(&gateway, &connection_id).len(), 1);
}

#[test]
fn a_projection_overtaken_by_a_full_replacement_is_not_published() {
    let navigation = Arc::new(CountingNavigation::default());
    let (gateway, connection_id) = navigation_gateway(navigation.clone());
    let first_read = navigation.reads.load(Ordering::SeqCst) + 1;
    navigation
        .overtake_with_full_replacement
        .store(true, Ordering::SeqCst);
    navigation
        .overtaken_read
        .store(first_read, Ordering::SeqCst);

    gateway.publish_committed_task_update(&entries_changed(), AppServerTime(3));

    let counts = published_task_counts(&gateway, &connection_id);
    assert!(
        !counts.contains(&first_read),
        "the overtaken read reached clients: {counts:?}"
    );
}

#[test]
fn a_projection_overtaken_by_a_direct_publication_is_not_published() {
    let logs = crate::logging::capture_test_logs();
    let navigation = Arc::new(CountingNavigation::default());
    let (gateway, connection_id) = navigation_gateway(navigation.clone());
    let first_read = navigation.reads.load(Ordering::SeqCst) + 1;
    navigation
        .overtaken_read
        .store(first_read, Ordering::SeqCst);

    gateway.publish_committed_task_update(&entries_changed(), AppServerTime(3));

    let counts = published_task_counts(&gateway, &connection_id);
    assert!(
        counts.iter().all(|count| *count != first_read),
        "the overtaken read reached clients: {counts:?}"
    );
    assert!(
        counts.windows(2).all(|pair| pair[0] < pair[1]),
        "entries moved backwards: {counts:?}"
    );
    assert!(logs
        .snapshot()
        .iter()
        .any(|line| line["event"] == json!("project_entries_projection_superseded")));
}
