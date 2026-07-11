use openaide_app_server_protocol::events::{AppServerEventPayload, EventScope};
use openaide_app_server_protocol::ids::TaskId;
use openaide_app_server_protocol::snapshot::{TaskNavigationSnapshot, TaskSnapshot};

use crate::client_lifecycle::AppServerTime;
use crate::client_lifecycle::ConnectionId;
use crate::server_requests::ServerRequestDelivery;
use crate::task_events::{CommittedTaskDelta, TaskUpdate};

use super::RpcGateway;
use crate::protocol_edge::{event_deliveries, GatewayEventDelivery};

impl RpcGateway {
    pub(crate) fn publish_task_update_by_id(
        &mut self,
        task_id: &TaskId,
        now: AppServerTime,
    ) -> Vec<GatewayEventDelivery> {
        self.publish_task_update(
            &TaskUpdate {
                task_id: task_id.as_str().to_string(),
                revision: 0,
                delta: None,
            },
            now,
        )
    }

    pub(crate) fn publish_task_update(
        &mut self,
        update: &TaskUpdate,
        now: AppServerTime,
    ) -> Vec<GatewayEventDelivery> {
        let task_id = TaskId::from(update.task_id.clone());
        let delta_events = self.publish_committed_task_delta(&task_id, update, now);
        if update.delta.is_some() {
            self.pending_event_deliveries.extend(delta_events.clone());
            return delta_events;
        }
        let Ok(task) = self.task_snapshots.open(&task_id) else {
            let events = self.publish_task_navigation_update(now).unwrap_or_default();
            self.pending_event_deliveries.extend(events.clone());
            return events;
        };
        let task = self.task_with_pending_requests(task);
        let events = self.publish_task_updates(&task, now);
        self.pending_event_deliveries.extend(events.clone());
        events
    }

    pub(crate) fn publish_task_update_for_connection(
        &mut self,
        connection_id: &ConnectionId,
        task_id: &TaskId,
        now: AppServerTime,
    ) -> (Vec<GatewayEventDelivery>, Vec<ServerRequestDelivery>) {
        self.publish_task_update_by_id(task_id, now);
        self.drain_published_task_update(connection_id, now)
    }

    pub(crate) fn publish_committed_task_update_for_connection(
        &mut self,
        connection_id: &ConnectionId,
        update: &TaskUpdate,
        now: AppServerTime,
    ) -> (Vec<GatewayEventDelivery>, Vec<ServerRequestDelivery>) {
        self.publish_task_update(update, now);
        self.drain_published_task_update(connection_id, now)
    }

    fn drain_published_task_update(
        &mut self,
        connection_id: &ConnectionId,
        now: AppServerTime,
    ) -> (Vec<GatewayEventDelivery>, Vec<ServerRequestDelivery>) {
        let events = self.drain_event_deliveries_for_connection(connection_id);
        let server_requests = self.drain_server_requests_for_connection(connection_id, now);
        (events, server_requests)
    }

    fn publish_committed_task_delta(
        &mut self,
        task_id: &TaskId,
        update: &TaskUpdate,
        now: AppServerTime,
    ) -> Vec<GatewayEventDelivery> {
        let Some(delta) = update.delta.clone() else {
            return Vec::new();
        };
        let payload = match delta {
            CommittedTaskDelta::ChatItemAppended { item } => {
                AppServerEventPayload::ChatItemAppended {
                    task_id: task_id.clone(),
                    item,
                }
            }
            CommittedTaskDelta::ChatItemChunk { message_id, chunk } => {
                AppServerEventPayload::ChatItemChunk {
                    task_id: task_id.clone(),
                    message_id,
                    chunk,
                }
            }
        };
        let client_hub = self.client_hub.clone();
        event_deliveries(self.state_stream.publish_committed(
            EventScope::Task {
                state_root_id: self.state_stream.state_root_id().clone(),
                task_id: task_id.clone(),
            },
            payload,
            |client_id| client_hub.delivery_for(client_id),
            now,
        ))
    }

    pub(crate) fn publish_task_updates(
        &mut self,
        task: &TaskSnapshot,
        now: AppServerTime,
    ) -> Vec<GatewayEventDelivery> {
        let client_hub = self.client_hub.clone();
        let task_update = self.state_stream.publish_committed(
            EventScope::StateRoot {
                state_root_id: self.state_stream.state_root_id().clone(),
            },
            AppServerEventPayload::TaskUpdated {
                task: task.task.clone(),
            },
            |client_id| client_hub.delivery_for(client_id),
            now,
        );
        let mut events = event_deliveries(task_update);
        let task_snapshot_update = self.state_stream.publish_committed(
            EventScope::Task {
                state_root_id: self.state_stream.state_root_id().clone(),
                task_id: task.task.task_id.clone(),
            },
            AppServerEventPayload::TaskSnapshotUpdated { task: task.clone() },
            |client_id| client_hub.delivery_for(client_id),
            now,
        );
        events.extend(event_deliveries(task_snapshot_update));
        events.extend(
            self.publish_project_collection_update(now)
                .unwrap_or_default(),
        );
        events.extend(
            self.publish_task_navigation_update(now)
                .into_iter()
                .flatten(),
        );
        events
    }

    pub(crate) fn publish_task_navigation_update(
        &mut self,
        now: AppServerTime,
    ) -> Option<Vec<GatewayEventDelivery>> {
        let snapshot = self.task_snapshots.list(false, None, None).ok()?;
        Some(self.publish_task_navigation_snapshot(
            TaskNavigationSnapshot {
                tasks: snapshot.tasks,
                active_task_id: None,
            },
            now,
        ))
    }

    pub(crate) fn publish_task_navigation_snapshot(
        &mut self,
        navigation: TaskNavigationSnapshot,
        now: AppServerTime,
    ) -> Vec<GatewayEventDelivery> {
        let client_hub = self.client_hub.clone();
        let outcome = self.state_stream.publish_committed(
            EventScope::StateRoot {
                state_root_id: self.state_stream.state_root_id().clone(),
            },
            AppServerEventPayload::TaskNavigationUpdated { navigation },
            |client_id| client_hub.delivery_for(client_id),
            now,
        );
        event_deliveries(outcome)
    }

    pub(crate) fn publish_project_collection_update(
        &mut self,
        now: AppServerTime,
    ) -> Option<Vec<GatewayEventDelivery>> {
        let projects = self.snapshots.project_collection_snapshot().ok()?;
        let client_hub = self.client_hub.clone();
        let outcome = self.state_stream.publish_committed(
            EventScope::StateRoot {
                state_root_id: self.state_stream.state_root_id().clone(),
            },
            AppServerEventPayload::ProjectCollectionUpdated { projects },
            |client_id| client_hub.delivery_for(client_id),
            now,
        );
        Some(event_deliveries(outcome))
    }
}
