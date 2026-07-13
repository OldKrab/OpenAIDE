use openaide_app_server_protocol::events::{
    AppServerEventPayload, EventScope, TaskChanges, TaskChatChange,
    TaskNavigationChange as ProtocolTaskNavigationChange,
};
use openaide_app_server_protocol::ids::TaskId;

use crate::client_lifecycle::{AppServerTime, ConnectionId};
use crate::server_requests::ServerRequestDelivery;
use crate::task_events::{
    CommittedChatChange, CommittedTaskChange, TaskNavigationChange, TaskUpdate, TaskUpdateKind,
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
        let events = match &update.kind {
            TaskUpdateKind::HistorySync(history_sync) => {
                self.publish_history_sync(&task_id, history_sync.clone(), now)
            }
            TaskUpdateKind::Changed(change) => {
                self.publish_committed_task_change(&task_id, update.revision, change, now)
            }
        };
        self.pending_event_deliveries.extend(events.clone());
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
        let task = self.task_snapshots.open_internal(task_id).ok();
        let changes = project_task_changes(task.as_ref(), change);
        let mut events = self.publish_task_payload(
            task_id,
            AppServerEventPayload::TaskChanged {
                task_id: task_id.clone(),
                revision,
                changes,
            },
            now,
        );

        let client_hub = self.client_hub.clone();
        for detail in &change.tool_details {
            events.extend(event_deliveries(self.state_stream.publish_committed(
                EventScope::Task {
                    state_root_id: self.state_stream.state_root_id().clone(),
                    task_id: task_id.clone(),
                },
                AppServerEventPayload::ToolDetailUpdated {
                    task_id: task_id.clone(),
                    artifact_id: detail.artifact_id.clone(),
                    details: detail.details.clone(),
                },
                |client_id| client_hub.delivery_for(client_id),
                now,
            )));
        }

        match (&change.navigation, task.as_ref()) {
            (TaskNavigationChange::Upsert, Some(task)) => {
                events.extend(self.publish_navigation_change(
                    ProtocolTaskNavigationChange::Upsert {
                        task: task.task.clone(),
                    },
                    now,
                ));
            }
            (TaskNavigationChange::Remove, _) => {
                events.extend(self.publish_navigation_change(
                    ProtocolTaskNavigationChange::Remove {
                        task_id: task_id.clone(),
                    },
                    now,
                ));
            }
            (TaskNavigationChange::None, _) => {}
            (TaskNavigationChange::Upsert, None) => {
                crate::logging::warn(
                    "task_change_navigation_projection_failed",
                    serde_json::json!({ "task_id": task_id.as_str() }),
                );
            }
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

fn project_task_changes(
    task: Option<&openaide_app_server_protocol::snapshot::TaskSnapshot>,
    committed: &CommittedTaskChange,
) -> TaskChanges {
    let fields = &committed.fields;
    TaskChanges {
        task: task
            .filter(|_| fields.summary)
            .map(|task| task.task.clone()),
        lifecycle: task.filter(|_| fields.lifecycle).map(|task| task.lifecycle),
        preparation: task
            .filter(|_| fields.preparation)
            .map(|task| task.preparation.clone()),
        agent_config: task
            .filter(|_| fields.agent_config)
            .map(|task| task.agent_config.clone()),
        agent_commands: task
            .filter(|_| fields.agent_commands)
            .map(|task| task.agent_commands.clone()),
        send_capability: fields
            .send_capability
            .then(|| task.map(|task| task.send_capability.clone()))
            .flatten(),
        chat: committed
            .chat
            .iter()
            .filter_map(|change| match change {
                CommittedChatChange::Append { item } => {
                    Some(TaskChatChange::Append { item: item.clone() })
                }
                CommittedChatChange::Upsert { item } => {
                    Some(TaskChatChange::Upsert { item: item.clone() })
                }
                CommittedChatChange::AppendText { message_id, text } => {
                    Some(TaskChatChange::AppendText {
                        message_id: message_id.clone(),
                        text: text.clone(),
                    })
                }
                CommittedChatChange::Replace => task.map(|task| TaskChatChange::Replace {
                    chat: task.chat.clone(),
                }),
            })
            .collect(),
        removed: fields.removed,
    }
}
