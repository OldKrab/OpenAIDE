use openaide_app_server_protocol::client::{
    ClientCapabilities, RequestedSurface, ShellDescriptor, ShellKind,
};
use openaide_app_server_protocol::events::{AppServerEventPayload, EventScope};
use openaide_app_server_protocol::ids::{ClientInstanceId, ProjectId, StateRootId, TaskId};
use openaide_app_server_protocol::snapshot::{
    AgentCollectionSnapshot, ProjectCollectionSnapshot, TaskHistorySyncSnapshot,
    TaskStatus as ProtocolTaskStatus, TaskSummary,
};
use openaide_app_server_protocol::state::SubscriptionScope;

use crate::client_lifecycle::{ClientContext, ConnectionId};
use crate::protocol::model::{IsolationKind, TaskStatus};
use crate::snapshots::{SnapshotBuilder, TaskNavigationSnapshotSource, TaskNavigationStore};
use crate::storage::records::{TaskPreparationRecord, TaskRecord};
use crate::storage::Store;

use super::*;

#[test]
fn subscribe_returns_snapshot_cursor_and_stores_subscription() {
    let mut stream = StateStream::new(StateRootId::from("root-1"));
    let snapshots = SnapshotBuilder::new("server-1".into(), "root-1".into());
    let result = stream
        .subscribe(
            &ctx("client-1", "conn-1"),
            SubscriptionScope::TaskNavigation { project_id: None },
            &snapshots,
            AppServerTime(1),
        )
        .unwrap();

    assert_eq!(result.cursor.as_str(), "cursor-0");
    assert_eq!(stream.subscription_count(), 1);
}

#[test]
fn first_event_after_subscribe_chains_from_subscription_cursor() {
    let mut stream = StateStream::new(StateRootId::from("root-1"));
    let snapshots = SnapshotBuilder::new("server-1".into(), "root-1".into());
    let subscribe = stream
        .subscribe(
            &ctx("client-1", "conn-1"),
            SubscriptionScope::TaskNavigation { project_id: None },
            &snapshots,
            AppServerTime(1),
        )
        .unwrap();

    let publish = stream.publish_committed(
        EventScope::StateRoot {
            state_root_id: StateRootId::from("root-1"),
        },
        AppServerEventPayload::TaskNavigationUpdated {
            navigation: openaide_app_server_protocol::snapshot::TaskNavigationSnapshot {
                tasks: Vec::new(),
                active_task_id: None,
            },
        },
        |client_id| {
            Some(Delivery {
                client_instance_id: client_id.clone(),
                connection_id: ConnectionId::new("conn-1"),
                request_capabilities: Vec::new(),
            })
        },
        AppServerTime(2),
    );

    assert_eq!(
        publish.deliveries[0].event.previous_cursor,
        subscribe.cursor
    );
    assert_eq!(publish.deliveries.len(), 1);
}

#[test]
fn filtered_events_keep_each_client_delivery_cursor_contiguous() {
    let mut stream = StateStream::new(StateRootId::from("root-1"));
    let snapshots = SnapshotBuilder::new("server-1".into(), "root-1".into());
    let client_one = stream
        .subscribe(
            &ctx("client-1", "conn-1"),
            SubscriptionScope::TaskNavigation { project_id: None },
            &snapshots,
            AppServerTime(1),
        )
        .unwrap();
    stream
        .subscribe(
            &ctx("client-2", "conn-2"),
            SubscriptionScope::Agents,
            &snapshots,
            AppServerTime(1),
        )
        .unwrap();

    let agent_event = stream.publish_committed(
        EventScope::StateRoot {
            state_root_id: StateRootId::from("root-1"),
        },
        AppServerEventPayload::AgentCollectionUpdated {
            agents: AgentCollectionSnapshot { agents: Vec::new() },
        },
        |client_id| Some(delivery(client_id)),
        AppServerTime(2),
    );
    assert_eq!(agent_event.deliveries.len(), 1);
    assert_eq!(
        agent_event.deliveries[0].delivery.client_instance_id,
        ClientInstanceId::from("client-2")
    );

    let navigation_event = stream.publish_committed(
        EventScope::StateRoot {
            state_root_id: StateRootId::from("root-1"),
        },
        AppServerEventPayload::TaskNavigationUpdated {
            navigation: openaide_app_server_protocol::snapshot::TaskNavigationSnapshot {
                tasks: Vec::new(),
                active_task_id: None,
            },
        },
        |client_id| Some(delivery(client_id)),
        AppServerTime(3),
    );

    assert_eq!(navigation_event.deliveries.len(), 1);
    assert_eq!(
        navigation_event.deliveries[0].event.previous_cursor,
        client_one.cursor
    );
}

#[test]
fn agent_collection_update_delivers_to_agent_subscribers() {
    let mut stream = StateStream::new(StateRootId::from("root-1"));
    let snapshots = SnapshotBuilder::new("server-1".into(), "root-1".into());
    stream
        .subscribe(
            &ctx("client-1", "conn-1"),
            SubscriptionScope::Agents,
            &snapshots,
            AppServerTime(1),
        )
        .unwrap();

    let publish = stream.publish_committed(
        EventScope::StateRoot {
            state_root_id: StateRootId::from("root-1"),
        },
        AppServerEventPayload::AgentCollectionUpdated {
            agents: AgentCollectionSnapshot { agents: Vec::new() },
        },
        |client_id| {
            Some(Delivery {
                client_instance_id: client_id.clone(),
                connection_id: ConnectionId::new("conn-1"),
                request_capabilities: Vec::new(),
            })
        },
        AppServerTime(2),
    );

    assert_eq!(publish.deliveries.len(), 1);
}

#[test]
fn project_collection_update_delivers_to_project_subscribers() {
    let mut stream = StateStream::new(StateRootId::from("root-1"));
    let snapshots = SnapshotBuilder::new("server-1".into(), "root-1".into());
    stream
        .subscribe(
            &ctx("client-1", "conn-1"),
            SubscriptionScope::Projects,
            &snapshots,
            AppServerTime(1),
        )
        .unwrap();

    let publish = stream.publish_committed(
        EventScope::StateRoot {
            state_root_id: StateRootId::from("root-1"),
        },
        AppServerEventPayload::ProjectCollectionUpdated {
            projects: ProjectCollectionSnapshot {
                projects: Vec::new(),
            },
        },
        |client_id| {
            Some(Delivery {
                client_instance_id: client_id.clone(),
                connection_id: ConnectionId::new("conn-1"),
                request_capabilities: Vec::new(),
            })
        },
        AppServerTime(2),
    );

    assert_eq!(publish.deliveries.len(), 1);
}

#[test]
fn event_matching_multiple_subscriptions_is_delivered_once_per_client() {
    let mut stream = StateStream::new(StateRootId::from("root-1"));
    let snapshots = SnapshotBuilder::new("server-1".into(), "root-1".into());
    let client = ctx("client-1", "conn-1");
    stream
        .subscribe(
            &client,
            SubscriptionScope::Projects,
            &snapshots,
            AppServerTime(1),
        )
        .unwrap();
    stream
        .subscribe(
            &client,
            SubscriptionScope::Agents,
            &snapshots,
            AppServerTime(1),
        )
        .unwrap();

    let publish = stream.publish_committed(
        EventScope::StateRoot {
            state_root_id: StateRootId::from("root-1"),
        },
        AppServerEventPayload::ProjectCollectionUpdated {
            projects: ProjectCollectionSnapshot {
                projects: Vec::new(),
            },
        },
        |client_id| Some(delivery(client_id)),
        AppServerTime(2),
    );

    assert_eq!(publish.deliveries.len(), 1);
}

#[test]
fn project_collection_update_advances_task_navigation_subscribers() {
    let mut stream = StateStream::new(StateRootId::from("root-1"));
    let snapshots = SnapshotBuilder::new("server-1".into(), "root-1".into());
    stream
        .subscribe(
            &ctx("client-1", "conn-1"),
            SubscriptionScope::TaskNavigation { project_id: None },
            &snapshots,
            AppServerTime(1),
        )
        .unwrap();

    let publish = stream.publish_committed(
        EventScope::StateRoot {
            state_root_id: StateRootId::from("root-1"),
        },
        AppServerEventPayload::ProjectCollectionUpdated {
            projects: ProjectCollectionSnapshot {
                projects: Vec::new(),
            },
        },
        |client_id| {
            Some(Delivery {
                client_instance_id: client_id.clone(),
                connection_id: ConnectionId::new("conn-1"),
                request_capabilities: Vec::new(),
            })
        },
        AppServerTime(2),
    );

    assert_eq!(publish.deliveries.len(), 1);
}

#[test]
fn project_collection_update_advances_task_subscribers() {
    let mut stream = StateStream::new(StateRootId::from("root-1"));
    let snapshots = SnapshotBuilder::new("server-1".into(), "root-1".into());
    stream
        .subscribe(
            &ctx("client-1", "conn-1"),
            SubscriptionScope::Task {
                task_id: "task-1".into(),
            },
            &snapshots,
            AppServerTime(1),
        )
        .unwrap();

    let publish = stream.publish_committed(
        EventScope::StateRoot {
            state_root_id: StateRootId::from("root-1"),
        },
        AppServerEventPayload::ProjectCollectionUpdated {
            projects: ProjectCollectionSnapshot {
                projects: Vec::new(),
            },
        },
        |client_id| {
            Some(Delivery {
                client_instance_id: client_id.clone(),
                connection_id: ConnectionId::new("conn-1"),
                request_capabilities: Vec::new(),
            })
        },
        AppServerTime(2),
    );

    assert_eq!(publish.deliveries.len(), 1);
}

#[test]
fn task_history_sync_update_delivers_to_matching_task_subscribers() {
    let mut stream = StateStream::new(StateRootId::from("root-1"));
    let snapshots = SnapshotBuilder::new("server-1".into(), "root-1".into());
    stream
        .subscribe(
            &ctx("client-1", "conn-1"),
            SubscriptionScope::Task {
                task_id: TaskId::from("task-1"),
            },
            &snapshots,
            AppServerTime(1),
        )
        .unwrap();

    let publish = stream.publish_committed(
        EventScope::Task {
            state_root_id: StateRootId::from("root-1"),
            task_id: TaskId::from("task-1"),
        },
        AppServerEventPayload::TaskHistorySyncUpdated {
            task_id: TaskId::from("task-1"),
            history_sync: TaskHistorySyncSnapshot::Checking { generation: 1 },
        },
        |client_id| Some(delivery(client_id)),
        AppServerTime(2),
    );

    assert_eq!(publish.deliveries.len(), 1);
}

#[test]
fn task_navigation_publication_reads_snapshot_after_durable_write() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let source = TaskNavigationStore::new(store.clone());
    let mut stream = StateStream::new(StateRootId::from("root-1"));
    let snapshots = SnapshotBuilder::with_task_navigation(
        "server-1".into(),
        "root-1".into(),
        std::sync::Arc::new(source.clone()),
    );
    stream
        .subscribe(
            &ctx("client-1", "conn-1"),
            SubscriptionScope::TaskNavigation { project_id: None },
            &snapshots,
            AppServerTime(1),
        )
        .unwrap();
    store.write_task(&task_record("task-1")).unwrap();

    let navigation = source.snapshot(None).unwrap();
    let publish = stream.publish_committed(
        EventScope::StateRoot {
            state_root_id: StateRootId::from("root-1"),
        },
        AppServerEventPayload::TaskNavigationUpdated { navigation },
        |client_id| {
            Some(Delivery {
                client_instance_id: client_id.clone(),
                connection_id: ConnectionId::new("conn-1"),
                request_capabilities: Vec::new(),
            })
        },
        AppServerTime(2),
    );

    let AppServerEventPayload::TaskNavigationUpdated { navigation } =
        publish.deliveries[0].event.payload.clone()
    else {
        panic!("expected task navigation event");
    };
    assert_eq!(navigation.tasks.len(), 1);
    assert_eq!(navigation.tasks[0].task_id.as_str(), "task-1");
    assert_eq!(publish.deliveries.len(), 1);
}

#[test]
fn full_task_navigation_update_skips_project_filtered_subscribers() {
    let mut stream = StateStream::new(StateRootId::from("root-1"));
    let snapshots = SnapshotBuilder::new("server-1".into(), "root-1".into());
    stream
        .subscribe(
            &ctx("client-1", "conn-1"),
            SubscriptionScope::TaskNavigation {
                project_id: Some(ProjectId::from("project-a")),
            },
            &snapshots,
            AppServerTime(1),
        )
        .unwrap();

    let publish = stream.publish_committed(
        EventScope::StateRoot {
            state_root_id: StateRootId::from("root-1"),
        },
        AppServerEventPayload::TaskNavigationUpdated {
            navigation: openaide_app_server_protocol::snapshot::TaskNavigationSnapshot {
                tasks: Vec::new(),
                active_task_id: None,
            },
        },
        |_| panic!("project-filtered subscriber should not receive full-list update"),
        AppServerTime(2),
    );

    assert!(publish.deliveries.is_empty());
}

#[test]
fn task_updated_delivers_to_project_filtered_task_navigation_subscribers() {
    let mut stream = StateStream::new(StateRootId::from("root-1"));
    let snapshots = SnapshotBuilder::new("server-1".into(), "root-1".into());
    stream
        .subscribe(
            &ctx("client-1", "conn-1"),
            SubscriptionScope::TaskNavigation {
                project_id: Some(ProjectId::from("project-a")),
            },
            &snapshots,
            AppServerTime(1),
        )
        .unwrap();

    let publish = stream.publish_committed(
        EventScope::StateRoot {
            state_root_id: StateRootId::from("root-1"),
        },
        AppServerEventPayload::TaskUpdated {
            task: TaskSummary {
                task_id: "task-1".into(),
                project_id: "project-a".into(),
                agent_id: "codex".into(),
                title: Some(openaide_app_server_protocol::snapshot::TaskTitle {
                    value: "Task".to_string(),
                    source: openaide_app_server_protocol::snapshot::TaskTitleSource::User,
                }),
                status: ProtocolTaskStatus::Idle,
                updated_at: "2026-01-01T00:00:00.000Z".to_string(),
                last_activity: "2026-01-01T00:00:00.000Z".to_string(),
                unread: false,
                has_messages: true,
            },
        },
        |client_id| {
            Some(Delivery {
                client_instance_id: client_id.clone(),
                connection_id: ConnectionId::new("conn-1"),
                request_capabilities: Vec::new(),
            })
        },
        AppServerTime(2),
    );

    assert_eq!(publish.deliveries.len(), 1);
}

#[test]
fn unsubscribed_scopes_receive_no_delivery() {
    let mut stream = StateStream::new(StateRootId::from("root-1"));
    let publish = stream.publish_committed(
        EventScope::StateRoot {
            state_root_id: StateRootId::from("root-1"),
        },
        AppServerEventPayload::TaskNavigationUpdated {
            navigation: openaide_app_server_protocol::snapshot::TaskNavigationSnapshot {
                tasks: Vec::new(),
                active_task_id: None,
            },
        },
        |_| panic!("no subscribers should be resolved"),
        AppServerTime(2),
    );

    assert!(publish.deliveries.is_empty());
}

#[test]
fn unsubscribe_removes_existing_subscription() {
    let mut stream = StateStream::new(StateRootId::from("root-1"));
    let client = ctx("client-1", "conn-1");
    let snapshots = SnapshotBuilder::new("server-1".into(), "root-1".into());
    let scope = SubscriptionScope::TaskNavigation { project_id: None };
    stream
        .subscribe(&client, scope.clone(), &snapshots, AppServerTime(1))
        .unwrap();
    stream.unsubscribe(&client, scope, AppServerTime(2));

    let publish = stream.publish_committed(
        EventScope::StateRoot {
            state_root_id: StateRootId::from("root-1"),
        },
        AppServerEventPayload::TaskNavigationUpdated {
            navigation: openaide_app_server_protocol::snapshot::TaskNavigationSnapshot {
                tasks: Vec::new(),
                active_task_id: None,
            },
        },
        |_| panic!("unsubscribed client should not be resolved"),
        AppServerTime(3),
    );

    assert!(publish.deliveries.is_empty());
}

#[test]
fn client_scoped_events_only_deliver_to_matching_client() {
    let mut stream = StateStream::new(StateRootId::from("root-1"));
    let snapshots = SnapshotBuilder::new("server-1".into(), "root-1".into());
    stream
        .subscribe(
            &ctx("client-1", "conn-1"),
            SubscriptionScope::Projects,
            &snapshots,
            AppServerTime(1),
        )
        .unwrap();
    stream
        .subscribe(
            &ctx("client-2", "conn-2"),
            SubscriptionScope::Projects,
            &snapshots,
            AppServerTime(1),
        )
        .unwrap();

    let publish = stream.publish_committed(
        EventScope::Client {
            state_root_id: StateRootId::from("root-1"),
            client_instance_id: ClientInstanceId::from("client-2"),
        },
        AppServerEventPayload::SnapshotReplaced {
            snapshot: snapshots
                .client_snapshot(
                    &ctx("client-2", "conn-2"),
                    RequestedSurface::Home,
                    &stream.read_token(),
                )
                .unwrap(),
        },
        |client_id| {
            Some(Delivery {
                client_instance_id: client_id.clone(),
                connection_id: ConnectionId::new(format!("conn-for-{}", client_id.as_str())),
                request_capabilities: Vec::new(),
            })
        },
        AppServerTime(2),
    );

    assert_eq!(publish.deliveries.len(), 1);
    assert_eq!(
        publish.deliveries[0].delivery.client_instance_id,
        ClientInstanceId::from("client-2")
    );
}

fn ctx(client_id: &str, connection_id: &str) -> ClientContext {
    ClientContext {
        client_instance_id: ClientInstanceId::from(client_id),
        connection_id: ConnectionId::new(connection_id),
        shell: ShellDescriptor {
            kind: ShellKind::Web,
            name: None,
            version: None,
        },
        requested_surface: RequestedSurface::Home,
        capabilities: ClientCapabilities::default(),
    }
}

fn delivery(client_id: &ClientInstanceId) -> Delivery {
    Delivery {
        client_instance_id: client_id.clone(),
        connection_id: ConnectionId::new(format!("conn-for-{}", client_id.as_str())),
        request_capabilities: Vec::new(),
    }
}

fn task_record(task_id: &str) -> TaskRecord {
    TaskRecord {
        task_id: task_id.to_string(),
        title: crate::storage::records::TaskTitle::new(
            "Task",
            crate::storage::records::TaskTitleSource::User,
        ),
        status: TaskStatus::Inactive,
        task_version: 1,
        message_history_version: 0,
        unread: false,
        created_at: "2026-01-01T00:00:00.000Z".to_string(),
        updated_at: "2026-01-01T00:00:00.000Z".to_string(),
        last_activity: "2026-01-01T00:00:00.000Z".to_string(),
        agent_id: "agent-a".to_string(),
        agent_name: "Agent A".to_string(),
        isolation: IsolationKind::Local,
        workspace_root: "/workspace/a".to_string(),
        lifecycle: crate::storage::records::TaskLifecycle::Visible,
        agent_session_id: None,
        active_turn_id: None,
        archived: false,
        tombstoned: false,
        revision: 1,
        config_options: Default::default(),
        config_options_catalog: None,
        config_mutation: Default::default(),
        agent_commands_catalog: None,
        model_id: None,
        preparation: TaskPreparationRecord::Ready,
    }
}
