use openaide_app_server_protocol::client::{
    ClientCapabilities, RequestedSurface, ShellDescriptor, ShellKind,
};
use openaide_app_server_protocol::events::{
    AppServerEventPayload, EventScope, TaskChanges, TaskNavigationChange,
};
use openaide_app_server_protocol::ids::{ClientInstanceId, StateRootId, TaskId};
use openaide_app_server_protocol::snapshot::{
    AgentCollectionSnapshot, ProjectCollectionSnapshot, TaskHistorySyncSnapshot,
};
use openaide_app_server_protocol::state::SubscriptionScope;

use crate::client_lifecycle::{ClientContext, ConnectionId};
use crate::snapshots::SnapshotBuilder;

use super::*;

#[test]
fn subscribe_returns_scope_baseline_and_registers_subscription() {
    let mut stream = StateStream::new(StateRootId::from("root-1"));
    let result = stream
        .subscribe(
            &ctx("client-1", "conn-1"),
            SubscriptionScope::TaskNavigation { project_id: None },
            &snapshots(),
            AppServerTime(1),
        )
        .unwrap();

    assert_eq!(result.cursor.as_str(), "cursor-0");
    assert_eq!(stream.subscription_count(), 1);
}

#[test]
fn unrelated_subscription_event_does_not_advance_task_cursor() {
    let mut stream = StateStream::new(StateRootId::from("root-1"));
    let context = ctx("client-1", "conn-1");
    let task_scope = SubscriptionScope::Task {
        task_id: TaskId::from("task-1"),
    };
    let task_baseline = stream
        .subscribe(&context, task_scope.clone(), &snapshots(), AppServerTime(1))
        .unwrap();
    stream
        .subscribe(
            &context,
            SubscriptionScope::Agents,
            &snapshots(),
            AppServerTime(1),
        )
        .unwrap();

    let agent_event = stream.publish_committed(
        state_root_scope(),
        AppServerEventPayload::AgentCollectionUpdated {
            agents: AgentCollectionSnapshot { agents: Vec::new() },
        },
        |client_id| Some(delivery(client_id)),
        AppServerTime(2),
    );
    assert_eq!(agent_event.deliveries.len(), 1);
    assert_eq!(
        agent_event.deliveries[0].event.subscription,
        SubscriptionScope::Agents
    );

    let task_event = stream.publish_committed(
        task_event_scope("task-1"),
        AppServerEventPayload::TaskChanged {
            task_id: TaskId::from("task-1"),
            revision: 2,
            changes: TaskChanges::default(),
        },
        |client_id| Some(delivery(client_id)),
        AppServerTime(3),
    );
    assert_eq!(task_event.deliveries.len(), 1);
    assert_eq!(
        task_event.deliveries[0].event.previous_cursor,
        task_baseline.cursor
    );
    assert_eq!(task_event.deliveries[0].event.cursor.as_str(), "cursor-1");
    assert_eq!(task_event.deliveries[0].event.subscription, task_scope);
}

#[test]
fn clients_sharing_one_scope_receive_the_same_cursor_link() {
    let mut stream = StateStream::new(StateRootId::from("root-1"));
    for client in ["client-1", "client-2"] {
        stream
            .subscribe(
                &ctx(client, &format!("conn-{client}")),
                SubscriptionScope::Projects,
                &snapshots(),
                AppServerTime(1),
            )
            .unwrap();
    }

    let publish = stream.publish_committed(
        state_root_scope(),
        AppServerEventPayload::ProjectCollectionUpdated {
            projects: ProjectCollectionSnapshot {
                projects: Vec::new(),
            },
        },
        |client_id| Some(delivery(client_id)),
        AppServerTime(2),
    );

    assert_eq!(publish.deliveries.len(), 2);
    assert!(publish
        .deliveries
        .iter()
        .all(
            |delivery| delivery.event.previous_cursor.as_str() == "cursor-0"
                && delivery.event.cursor.as_str() == "cursor-1"
                && delivery.event.subscription == SubscriptionScope::Projects
        ));
}

#[test]
fn project_updates_do_not_leak_into_navigation_or_task_subscriptions() {
    let mut stream = StateStream::new(StateRootId::from("root-1"));
    let context = ctx("client-1", "conn-1");
    for scope in [
        SubscriptionScope::Projects,
        SubscriptionScope::TaskNavigation { project_id: None },
        SubscriptionScope::Task {
            task_id: TaskId::from("task-1"),
        },
    ] {
        stream
            .subscribe(&context, scope, &snapshots(), AppServerTime(1))
            .unwrap();
    }

    let publish = stream.publish_committed(
        state_root_scope(),
        AppServerEventPayload::ProjectCollectionUpdated {
            projects: ProjectCollectionSnapshot {
                projects: Vec::new(),
            },
        },
        |client_id| Some(delivery(client_id)),
        AppServerTime(2),
    );

    assert_eq!(publish.deliveries.len(), 1);
    assert_eq!(
        publish.deliveries[0].event.subscription,
        SubscriptionScope::Projects
    );
}

#[test]
fn history_sync_uses_the_matching_task_stream() {
    let mut stream = StateStream::new(StateRootId::from("root-1"));
    let scope = SubscriptionScope::Task {
        task_id: TaskId::from("task-1"),
    };
    stream
        .subscribe(
            &ctx("client-1", "conn-1"),
            scope.clone(),
            &snapshots(),
            AppServerTime(1),
        )
        .unwrap();

    let publish = stream.publish_committed(
        task_event_scope("task-1"),
        AppServerEventPayload::TaskHistorySyncUpdated {
            task_id: TaskId::from("task-1"),
            history_sync: TaskHistorySyncSnapshot::Syncing { generation: 1 },
        },
        |client_id| Some(delivery(client_id)),
        AppServerTime(2),
    );

    assert_eq!(publish.deliveries.len(), 1);
    assert_eq!(publish.deliveries[0].event.subscription, scope);
}

#[test]
fn navigation_change_is_delivered_only_to_navigation_scope() {
    let mut stream = StateStream::new(StateRootId::from("root-1"));
    stream
        .subscribe(
            &ctx("client-1", "conn-1"),
            SubscriptionScope::TaskNavigation { project_id: None },
            &snapshots(),
            AppServerTime(1),
        )
        .unwrap();

    let publish = stream.publish_committed(
        state_root_scope(),
        AppServerEventPayload::TaskNavigationChanged {
            change: TaskNavigationChange::Remove {
                task_id: TaskId::from("task-1"),
            },
        },
        |client_id| Some(delivery(client_id)),
        AppServerTime(2),
    );

    assert_eq!(publish.deliveries.len(), 1);
    assert!(matches!(
        publish.deliveries[0].event.subscription,
        SubscriptionScope::TaskNavigation { .. }
    ));
}

#[test]
fn unsubscribe_removes_only_the_requested_scope() {
    let mut stream = StateStream::new(StateRootId::from("root-1"));
    let context = ctx("client-1", "conn-1");
    stream
        .subscribe(
            &context,
            SubscriptionScope::Projects,
            &snapshots(),
            AppServerTime(1),
        )
        .unwrap();
    stream.unsubscribe(&context, SubscriptionScope::Projects, AppServerTime(2));

    let publish = stream.publish_committed(
        state_root_scope(),
        AppServerEventPayload::ProjectCollectionUpdated {
            projects: ProjectCollectionSnapshot {
                projects: Vec::new(),
            },
        },
        |client_id| Some(delivery(client_id)),
        AppServerTime(3),
    );
    assert!(publish.deliveries.is_empty());
}

fn snapshots() -> SnapshotBuilder {
    SnapshotBuilder::new("server-1".into(), "root-1".into())
}

fn state_root_scope() -> EventScope {
    EventScope::StateRoot {
        state_root_id: StateRootId::from("root-1"),
    }
}

fn task_event_scope(task_id: &str) -> EventScope {
    EventScope::Task {
        state_root_id: StateRootId::from("root-1"),
        task_id: TaskId::from(task_id),
    }
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
