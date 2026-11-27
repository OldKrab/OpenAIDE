use openaide_app_server_protocol::client::{
    ClientCapabilities, RequestedSurface, ShellDescriptor, ShellKind,
};
use openaide_app_server_protocol::events::{AppServerEventPayload, TaskChanges};
use openaide_app_server_protocol::ids::{ClientInstanceId, TaskId};
use openaide_app_server_protocol::snapshot::TaskHistorySyncSnapshot;
use openaide_app_server_protocol::state::SubscriptionScope;

use crate::client_lifecycle::{ClientContext, ConnectionId};
use crate::snapshots::SnapshotBuilder;

use super::*;

#[test]
fn a_view_subscription_is_registered_once() {
    let mut stream = StateStream::new();
    let context = ctx("client-1", "conn-1");
    stream.register_view_subscription(&context, nav_scope());
    stream.register_view_subscription(&context, nav_scope());

    assert_eq!(stream.subscription_count(), 1);
}

#[test]
fn a_task_event_reaches_only_that_tasks_subscribers() {
    let mut stream = StateStream::new();
    let context = ctx("client-1", "conn-1");
    stream.register_view_subscription(&context, nav_scope());
    for scope in [task_scope("task-1"), task_scope("task-2")] {
        stream
            .subscribe(&context, scope, &snapshots(), AppServerTime(1))
            .unwrap();
    }

    let publish = publish_task(&mut stream, task_changed("task-1"));

    assert_eq!(publish.deliveries.len(), 1);
    assert_eq!(
        publish.deliveries[0].event.subscription,
        task_scope("task-1")
    );
}

#[test]
fn clients_sharing_one_scope_all_receive_the_event() {
    let mut stream = StateStream::new();
    for client in ["client-1", "client-2"] {
        stream
            .subscribe(
                &ctx(client, &format!("conn-{client}")),
                task_scope("task-1"),
                &snapshots(),
                AppServerTime(1),
            )
            .unwrap();
    }

    let publish = publish_task(&mut stream, task_changed("task-1"));

    assert_eq!(publish.deliveries.len(), 2);
    assert!(publish
        .deliveries
        .iter()
        .all(|delivery| delivery.event.subscription == task_scope("task-1")));
}

#[test]
fn resubscribing_keeps_one_subscription() {
    let mut stream = StateStream::new();
    let context = ctx("client-1", "conn-1");
    for _ in 0..2 {
        stream
            .subscribe(
                &context,
                task_scope("task-1"),
                &snapshots(),
                AppServerTime(1),
            )
            .unwrap();
    }

    let publish = publish_task(&mut stream, task_changed("task-1"));

    assert_eq!(stream.subscription_count(), 1);
    assert_eq!(publish.deliveries.len(), 1);
}

#[test]
fn a_subscriber_with_no_route_is_skipped() {
    let mut stream = StateStream::new();
    stream
        .subscribe(
            &ctx("client-1", "conn-1"),
            task_scope("task-1"),
            &snapshots(),
            AppServerTime(1),
        )
        .unwrap();

    let publish = stream.publish(
        EventAddress::Subscribers(task_scope("task-1")),
        task_changed("task-1"),
        |_| None,
        AppServerTime(2),
    );

    assert!(publish.deliveries.is_empty());
}

#[test]
fn history_sync_reaches_the_task_subscription() {
    let mut stream = StateStream::new();
    let scope = task_scope("task-1");
    stream
        .subscribe(
            &ctx("client-1", "conn-1"),
            scope.clone(),
            &snapshots(),
            AppServerTime(1),
        )
        .unwrap();

    let publish = publish_task(
        &mut stream,
        AppServerEventPayload::TaskHistorySyncUpdated {
            task_id: TaskId::from("task-1"),
            history_sync: TaskHistorySyncSnapshot::Syncing { generation: 1 },
        },
    );

    assert_eq!(publish.deliveries.len(), 1);
    assert_eq!(publish.deliveries[0].event.subscription, scope);
}

#[test]
fn a_navigation_frame_reaches_only_the_client_whose_view_it_is() {
    let mut stream = StateStream::new();
    for client in ["client-1", "client-2"] {
        stream.register_view_subscription(&ctx(client, &format!("conn-{client}")), nav_scope());
    }

    let publish = stream.publish(
        EventAddress::Client {
            client: ClientInstanceId::from("client-1"),
            scope: nav_scope(),
        },
        nav_frame(),
        |client_id| Some(delivery(client_id)),
        AppServerTime(2),
    );

    assert_eq!(publish.deliveries.len(), 1);
    assert_eq!(
        publish.deliveries[0].delivery.client_instance_id.as_str(),
        "client-1"
    );
}

#[test]
fn a_removed_task_ends_its_task_subscriptions_after_the_last_event() {
    let mut stream = StateStream::new();
    let context = ctx("client-1", "conn-1");
    stream.register_view_subscription(&context, nav_scope());
    for scope in [task_scope("task-1"), task_scope("task-2")] {
        stream
            .subscribe(&context, scope, &snapshots(), AppServerTime(1))
            .unwrap();
    }

    let removed = publish_task(
        &mut stream,
        AppServerEventPayload::TaskChanged {
            task_id: TaskId::from("task-1"),
            revision: 3,
            changes: TaskChanges {
                removed: true,
                ..Default::default()
            },
        },
    );
    let after = publish_task(&mut stream, task_changed("task-1"));

    assert_eq!(removed.deliveries.len(), 1);
    assert!(after.deliveries.is_empty());
    // The Navigation view and the other Task are untouched.
    assert_eq!(stream.subscription_count(), 2);
}

#[test]
fn a_removed_tasks_content_removal_ends_only_its_entity_subscriptions() {
    let mut stream = StateStream::new();
    let context = ctx("client-1", "conn-1");
    let history = SubscriptionScope::SubagentHistory {
        task_id: TaskId::from("task-1"),
        subagent_id: "subagent-1".into(),
    };
    let tool = SubscriptionScope::ToolDetail {
        task_id: TaskId::from("task-1"),
        artifact_id: "artifact-1".to_string(),
    };
    let other_history = SubscriptionScope::SubagentHistory {
        task_id: TaskId::from("task-2"),
        subagent_id: "subagent-1".into(),
    };
    // Registered directly: the bare snapshot builder cannot serve history or Tool detail.
    for scope in [task_scope("task-1"), history, tool, other_history] {
        stream.register_view_subscription(&context, scope);
    }

    let publish = publish_task(
        &mut stream,
        AppServerEventPayload::TaskContentRemoved {
            task_id: TaskId::from("task-1"),
        },
    );

    assert_eq!(publish.deliveries.len(), 2);
    // The Task's own subscription ends with its TaskChanged, not with this event.
    assert_eq!(stream.subscription_count(), 2);
}

#[test]
fn unsubscribe_removes_only_the_requested_scope() {
    let mut stream = StateStream::new();
    let context = ctx("client-1", "conn-1");
    for scope in [task_scope("task-1"), task_scope("task-2")] {
        stream
            .subscribe(&context, scope, &snapshots(), AppServerTime(1))
            .unwrap();
    }
    stream.unsubscribe(&context, task_scope("task-1"), AppServerTime(2));

    let publish = publish_task(&mut stream, task_changed("task-1"));

    assert!(publish.deliveries.is_empty());
    assert_eq!(stream.subscription_count(), 1);
}

fn publish_task(stream: &mut StateStream, payload: AppServerEventPayload) -> PublishOutcome {
    let address = EventAddress::of_task_payload(&payload).expect("a Task-owned payload");
    stream.publish(
        address,
        payload,
        |client_id| Some(delivery(client_id)),
        AppServerTime(2),
    )
}

fn task_changed(task_id: &str) -> AppServerEventPayload {
    AppServerEventPayload::TaskChanged {
        task_id: TaskId::from(task_id),
        revision: 2,
        changes: TaskChanges::default(),
    }
}

fn task_scope(task_id: &str) -> SubscriptionScope {
    SubscriptionScope::Task {
        task_id: TaskId::from(task_id),
    }
}

fn nav_scope() -> SubscriptionScope {
    SubscriptionScope::TaskNavigation {
        section: openaide_app_server_protocol::task::TaskNavigationSection::Tasks,
    }
}

fn nav_frame() -> AppServerEventPayload {
    AppServerEventPayload::NavigationFrame {
        frame: openaide_app_server_protocol::navigation::NavigationFrame {
            section: openaide_app_server_protocol::task::TaskNavigationSection::Tasks,
            revision: 1,
            group_order: None,
            removed_groups: Vec::new(),
            groups: Default::default(),
        },
    }
}

fn snapshots() -> SnapshotBuilder {
    SnapshotBuilder::new("server-1".into(), "root-1".into())
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
