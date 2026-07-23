use openaide_app_server_protocol::events::{
    AppServerEventPayload, EventScope, TaskNavigationChange as ProtocolTaskNavigationChange,
};
use openaide_app_server_protocol::ids::TaskId;

use crate::client_lifecycle::{AppServerTime, ConnectionId};
use crate::server_requests::ServerRequestDelivery;
use crate::task_events::{CommittedTaskChange, TaskUpdate, TaskUpdateKind};

use super::RpcGateway;
use crate::protocol_edge::{event_deliveries, GatewayEventDelivery};

impl RpcGateway {
    pub(crate) fn publish_task_update(
        &mut self,
        update: &TaskUpdate,
        now: AppServerTime,
    ) -> Vec<GatewayEventDelivery> {
        let task_id = TaskId::from(update.task_id.clone());
        let mut events = match &update.kind {
            TaskUpdateKind::NavigationChanged => self.publish_navigation_replacement(now),
            TaskUpdateKind::HistorySync(history_sync) => {
                self.publish_history_sync(&task_id, history_sync.clone(), now)
            }
            TaskUpdateKind::Changed(change) => {
                self.publish_committed_task_change(&task_id, update.revision, change, now)
            }
            TaskUpdateKind::ToolDetailChanged {
                artifact_id,
                deltas,
            } => self.publish_tool_detail_change(
                &task_id,
                artifact_id,
                update.revision,
                deltas.clone(),
                now,
            ),
        };
        let pending_requests = self.server_requests.pending_for_task(&task_id);
        if !pending_requests.is_empty() {
            // Permission/question opening precedes the Task mutation that marks
            // it waiting. Publish broker state through the same ordered Task
            // stream so live clients do not depend on reverse-RPC delivery.
            events.extend(self.publish_task_payload(
                &task_id,
                AppServerEventPayload::TaskRequestsUpdated {
                    task_id: task_id.clone(),
                    requests: pending_requests,
                },
                now,
            ));
        }
        self.pending_event_deliveries.extend(events.clone());
        events
    }

    pub(crate) fn publish_navigation_replacement(
        &mut self,
        now: AppServerTime,
    ) -> Vec<GatewayEventDelivery> {
        let Ok(navigation) = self.snapshots.task_navigation_snapshot() else {
            return Vec::new();
        };
        let client_hub = self.client_hub.clone();
        event_deliveries(self.state_stream.publish_committed(
            EventScope::StateRoot {
                state_root_id: self.state_stream.state_root_id().clone(),
            },
            AppServerEventPayload::TaskNavigationReplaced { navigation },
            |client_id| client_hub.delivery_for(client_id),
            now,
        ))
    }

    pub(crate) fn publish_committed_task_update_for_connection(
        &mut self,
        connection_id: &ConnectionId,
        update: &TaskUpdate,
        now: AppServerTime,
    ) -> (Vec<GatewayEventDelivery>, Vec<ServerRequestDelivery>) {
        self.publish_task_update(update, now);
        let events = self.drain_event_deliveries_for_connection(connection_id);
        let server_requests = self.drain_server_requests_for_connection(connection_id, now);
        (events, server_requests)
    }

    fn publish_history_sync(
        &mut self,
        task_id: &TaskId,
        history_sync: openaide_app_server_protocol::snapshot::TaskHistorySyncSnapshot,
        now: AppServerTime,
    ) -> Vec<GatewayEventDelivery> {
        self.publish_task_payload(
            task_id,
            AppServerEventPayload::TaskHistorySyncUpdated {
                task_id: task_id.clone(),
                history_sync,
            },
            now,
        )
    }

    fn publish_committed_task_change(
        &mut self,
        task_id: &TaskId,
        revision: u64,
        change: &CommittedTaskChange,
        now: AppServerTime,
    ) -> Vec<GatewayEventDelivery> {
        let mut events = self.publish_task_payload(
            task_id,
            AppServerEventPayload::TaskChanged {
                task_id: task_id.clone(),
                revision,
                changes: change.changes.clone(),
            },
            now,
        );

        let client_hub = self.client_hub.clone();
        for detail in &change.tool_details {
            let mut deltas = vec![
                openaide_app_server_protocol::events::ToolDetailDelta::ReplaceDetails {
                    details: Box::new(detail.details.clone()),
                },
            ];
            deltas.extend(detail.terminal_appends.iter().map(|append| {
                openaide_app_server_protocol::events::ToolDetailDelta::AppendTerminal {
                    terminal_id: append.terminal_id.clone(),
                    data: append.data.clone(),
                }
            }));
            events.extend(event_deliveries(self.state_stream.publish_committed(
                EventScope::Task {
                    state_root_id: self.state_stream.state_root_id().clone(),
                    task_id: task_id.clone(),
                },
                AppServerEventPayload::ToolDetailChanged {
                    task_id: task_id.clone(),
                    artifact_id: detail.artifact_id.clone(),
                    revision: detail.details.revision,
                    deltas,
                },
                |client_id| client_hub.delivery_for(client_id),
                now,
            )));
        }

        if let Some(navigation) = &change.navigation {
            events.extend(self.publish_navigation_change(navigation.clone(), now));
        }
        if let Some(lifecycle) = &change.lifecycle {
            let client_hub = self.client_hub.clone();
            events.extend(event_deliveries(self.state_stream.publish_committed(
                EventScope::StateRoot {
                    state_root_id: self.state_stream.state_root_id().clone(),
                },
                AppServerEventPayload::TaskLifecycleChanged {
                    change: lifecycle.clone(),
                },
                |client_id| client_hub.delivery_for(client_id),
                now,
            )));
        }
        events
    }

    fn publish_task_payload(
        &mut self,
        task_id: &TaskId,
        payload: AppServerEventPayload,
        now: AppServerTime,
    ) -> Vec<GatewayEventDelivery> {
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

    fn publish_tool_detail_change(
        &mut self,
        task_id: &TaskId,
        artifact_id: &str,
        revision: u64,
        deltas: Vec<openaide_app_server_protocol::events::ToolDetailDelta>,
        now: AppServerTime,
    ) -> Vec<GatewayEventDelivery> {
        let client_hub = self.client_hub.clone();
        event_deliveries(self.state_stream.publish_committed(
            EventScope::Task {
                state_root_id: self.state_stream.state_root_id().clone(),
                task_id: task_id.clone(),
            },
            AppServerEventPayload::ToolDetailChanged {
                task_id: task_id.clone(),
                artifact_id: artifact_id.to_string(),
                revision,
                deltas,
            },
            |client_id| client_hub.delivery_for(client_id),
            now,
        ))
    }

    fn publish_navigation_change(
        &mut self,
        change: ProtocolTaskNavigationChange,
        now: AppServerTime,
    ) -> Vec<GatewayEventDelivery> {
        let client_hub = self.client_hub.clone();
        event_deliveries(self.state_stream.publish_committed(
            EventScope::StateRoot {
                state_root_id: self.state_stream.state_root_id().clone(),
            },
            AppServerEventPayload::TaskNavigationChanged { change },
            |client_id| client_hub.delivery_for(client_id),
            now,
        ))
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
