use openaide_app_server_protocol::client::{
    ClientCapabilities, RequestedSurface, ShellDescriptor, ShellKind,
};
use openaide_app_server_protocol::events::{AppServerEventPayload, EventScope, TaskChanges};
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
            SubscriptionScope::TaskNavigation {
                section: openaide_app_server_protocol::task::TaskNavigationSection::Tasks,
                project_ids: None,
            },
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
    // Cursor values come from one sequence that every publication advances.
    assert_eq!(task_event.deliveries[0].event.cursor.as_str(), "cursor-2");
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

fn agents_updated() -> AppServerEventPayload {
    AppServerEventPayload::AgentCollectionUpdated {
        agents: AgentCollectionSnapshot { agents: Vec::new() },
    }
}

#[test]
fn an_event_for_one_client_leaves_another_clients_chain_unbroken() {
    let mut stream = StateStream::new(StateRootId::from("root-1"));
    let mut baselines = Vec::new();
    for client in ["client-1", "client-2"] {
        baselines.push(
            stream
                .subscribe(
                    &ctx(client, &format!("conn-{client}")),
                    SubscriptionScope::Agents,
                    &snapshots(),
                    AppServerTime(1),
                )
                .unwrap()
                .cursor,
        );
    }

    let private = stream.publish_committed(
        EventScope::Client {
            state_root_id: StateRootId::from("root-1"),
            client_instance_id: ClientInstanceId::from("client-1"),
        },
        agents_updated(),
        |client_id| Some(delivery(client_id)),
        AppServerTime(2),
    );
    let shared = stream.publish_committed(
        state_root_scope(),
        agents_updated(),
        |client_id| Some(delivery(client_id)),
        AppServerTime(3),
    );

    assert_eq!(private.deliveries.len(), 1);
    let link = |client: &str| {
        shared
            .deliveries
            .iter()
            .find(|delivery| delivery.delivery.client_instance_id.as_str() == client)
            .map(|delivery| delivery.event.previous_cursor.clone())
            .unwrap()
    };
    assert_eq!(link("client-1"), private.deliveries[0].event.cursor);
    assert_eq!(link("client-2"), baselines[1]);
}

#[test]
fn resubscribing_rebases_the_chain_on_the_new_baseline() {
    let mut stream = StateStream::new(StateRootId::from("root-1"));
    let context = ctx("client-1", "conn-1");
    stream
        .subscribe(
            &context,
            SubscriptionScope::Agents,
            &snapshots(),
            AppServerTime(1),
        )
        .unwrap();
    let before = stream.publish_committed(
        state_root_scope(),
        agents_updated(),
        |client_id| Some(delivery(client_id)),
        AppServerTime(2),
    );

    let baseline = stream
        .subscribe(
            &context,
            SubscriptionScope::Agents,
            &snapshots(),
            AppServerTime(3),
        )
        .unwrap()
        .cursor;
    let after = stream.publish_committed(
        state_root_scope(),
        agents_updated(),
        |client_id| Some(delivery(client_id)),
        AppServerTime(4),
    );

    assert_eq!(stream.subscription_count(), 1);
    // An event already sent for the old baseline is at or before the new one.
    assert_eq!(before.deliveries[0].event.cursor, baseline);
    assert_eq!(after.deliveries[0].event.previous_cursor, baseline);
}

#[test]
fn an_undeliverable_event_still_moves_the_chain_so_the_client_sees_the_gap() {
    let mut stream = StateStream::new(StateRootId::from("root-1"));
    let baseline = stream
        .subscribe(
            &ctx("client-1", "conn-1"),
            SubscriptionScope::Agents,
            &snapshots(),
            AppServerTime(1),
        )
        .unwrap()
        .cursor;

    let missed = stream.publish_committed(
        state_root_scope(),
        agents_updated(),
        |_| None,
        AppServerTime(2),
    );
    let next = stream.publish_committed(
        state_root_scope(),
        agents_updated(),
        |client_id| Some(delivery(client_id)),
        AppServerTime(3),
    );

    assert!(missed.deliveries.is_empty());
    assert_ne!(next.deliveries[0].event.previous_cursor, baseline);
}

#[test]
fn project_updates_do_not_leak_into_navigation_or_task_subscriptions() {
    let mut stream = StateStream::new(StateRootId::from("root-1"));
    let context = ctx("client-1", "conn-1");
    for scope in [
        SubscriptionScope::Projects,
        SubscriptionScope::TaskNavigation {
            section: openaide_app_server_protocol::task::TaskNavigationSection::Tasks,
            project_ids: None,
        },
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
            SubscriptionScope::TaskNavigation {
                section: openaide_app_server_protocol::task::TaskNavigationSection::Tasks,
                project_ids: None,
            },
            &snapshots(),
            AppServerTime(1),
        )
        .unwrap();

    let publish = stream.publish_committed(
        state_root_scope(),
        AppServerEventPayload::RefreshStateChanged {
            refresh: openaide_app_server_protocol::snapshot::TaskNavigationRefreshState::Refreshing,
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
        role: Default::default(),
    }
}

fn delivery(client_id: &ClientInstanceId) -> Delivery {
    Delivery {
        client_instance_id: client_id.clone(),
        connection_id: ConnectionId::new(format!("conn-for-{}", client_id.as_str())),
    }
}
