use openaide_app_server_protocol::events::{AppServerEventPayload, EventScope};
use openaide_app_server_protocol::ids::{ProjectId, TaskId};
use openaide_app_server_protocol::task::{TaskNavigationSection, TaskNavigationSection::*};

use crate::client_lifecycle::{AppServerTime, ConnectionId};
use crate::server_requests::ServerRequestDelivery;
use crate::task_events::{
    CommittedNavigationChange, CommittedTaskChange, TaskUpdate, TaskUpdateKind,
};

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
            TaskUpdateKind::NavigationProjectEntriesChanged { project_id } => {
                self.publish_project_entries_replaced(project_id, now)
            }
            TaskUpdateKind::NavigationRefreshStateChanged { refresh } => {
                self.publish_refresh_state_changed(refresh.clone(), now)
            }
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
        let mut events = Vec::new();
        for section in [TaskNavigationSection::Tasks, TaskNavigationSection::Archive] {
            let Ok(navigation) = self.snapshots.task_navigation_snapshot(section, None) else {
                continue;
            };
            let client_hub = self.client_hub.clone();
            events.extend(event_deliveries(self.state_stream.publish_committed(
                EventScope::StateRoot {
                    state_root_id: self.state_stream.state_root_id().clone(),
                },
                AppServerEventPayload::NavigationReplaced { navigation },
                |client_id| client_hub.delivery_for(client_id),
                now,
            )));
        }
        events
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
            events.extend(match navigation {
                CommittedNavigationChange::TaskUpdated(task) => {
                    self.publish_task_updated(task.clone(), now)
                }
                CommittedNavigationChange::ProjectEntriesChanged { project_id } => {
                    self.publish_project_entries_replaced(project_id, now)
                }
            });
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

    fn publish_task_updated(
        &mut self,
        task: Box<openaide_app_server_protocol::snapshot::TaskSummary>,
        now: AppServerTime,
    ) -> Vec<GatewayEventDelivery> {
        let payload = AppServerEventPayload::TaskUpdated {
            project_id: task.project_id.clone(),
            task,
        };
        let client_hub = self.client_hub.clone();
        event_deliveries(self.state_stream.publish_committed(
            EventScope::StateRoot {
                state_root_id: self.state_stream.state_root_id().clone(),
            },
            payload,
            |client_id| client_hub.delivery_for(client_id),
            now,
        ))
    }

    fn publish_project_entries_replaced(
        &mut self,
        project_id: &ProjectId,
        now: AppServerTime,
    ) -> Vec<GatewayEventDelivery> {
        let mut events = Vec::new();
        for section in [Tasks, Archive] {
            let selected = [project_id.clone()];
            let Ok(snapshot) = self
                .snapshots
                .task_navigation_snapshot(section, Some(&selected))
            else {
                continue;
            };
            let group = snapshot.groups.into_iter().next();
            let (task_count, entries, has_more) = group
                .map(|group| (group.task_count, group.entries, group.has_more))
                .unwrap_or((0, Vec::new(), false));
            let client_hub = self.client_hub.clone();
            events.extend(event_deliveries(self.state_stream.publish_committed(
                EventScope::StateRoot {
                    state_root_id: self.state_stream.state_root_id().clone(),
                },
                AppServerEventPayload::ProjectEntriesReplaced {
                    section,
                    project_id: project_id.clone(),
                    task_count,
                    entries,
                    has_more,
                },
                |client_id| client_hub.delivery_for(client_id),
                now,
            )));
        }
        events
    }

    fn publish_refresh_state_changed(
        &mut self,
        refresh: openaide_app_server_protocol::snapshot::TaskNavigationRefreshState,
        now: AppServerTime,
    ) -> Vec<GatewayEventDelivery> {
        let client_hub = self.client_hub.clone();
        event_deliveries(self.state_stream.publish_committed(
            EventScope::StateRoot {
                state_root_id: self.state_stream.state_root_id().clone(),
            },
            AppServerEventPayload::RefreshStateChanged { refresh },
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
        let mut events = event_deliveries(outcome);
        // Project membership and labels are part of both Navigation sections.
        events.extend(self.publish_navigation_replacement(now));
        Some(events)
    }
}
