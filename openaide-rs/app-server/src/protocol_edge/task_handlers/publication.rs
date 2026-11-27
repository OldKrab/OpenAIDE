use openaide_app_server_protocol::events::AppServerEventPayload;
use openaide_app_server_protocol::ids::TaskId;
use openaide_app_server_protocol::snapshot::TaskStatus;

use crate::client_lifecycle::AppServerTime;
use crate::task_events::{CommittedTaskChange, TaskUpdate, TaskUpdateKind};

use super::RpcGateway;
use crate::protocol_edge::{event_deliveries, GatewayEventDelivery};
use crate::state_sync::EventAddress;

impl RpcGateway {
    /// Publishes one committed Task update to its subscribers and the outbox. Navigation is
    /// not published here: commits mark it dirty and the flush emits its frames.
    pub(crate) fn publish_task_update(
        &mut self,
        update: &TaskUpdate,
        now: AppServerTime,
    ) -> Vec<GatewayEventDelivery> {
        let task_id = TaskId::from(update.task_id.clone());
        let mut events = match &update.kind {
            TaskUpdateKind::NavigationDirty => Vec::new(),
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
            TaskUpdateKind::SubagentCatalogChanged { catalog } => self.publish_task_payload(
                AppServerEventPayload::SubagentCatalogUpdated {
                    catalog: catalog.clone(),
                },
                now,
            ),
            TaskUpdateKind::SubagentHistoryChanged {
                subagent_id,
                chat,
                current_plan,
            } => self.publish_task_payload(
                AppServerEventPayload::SubagentHistoryChanged {
                    task_id: task_id.clone(),
                    subagent_id: subagent_id.clone(),
                    revision: update.revision,
                    chat: chat.clone(),
                    current_plan: current_plan.clone(),
                },
                now,
            ),
        };
        let pending_requests = self.server_requests.pending_for_task(&task_id);
        let clears_transient_requests = matches!(
            &update.kind,
            TaskUpdateKind::Changed(change)
                if change.changes.task.as_ref().is_some_and(|task| matches!(
                    task.status,
                    TaskStatus::Stopping
                        | TaskStatus::Idle
                        | TaskStatus::Interrupted
                        | TaskStatus::Failed
                        | TaskStatus::Completed
                ))
        );
        // A request can open and resolve between two Task updates without a client response,
        // as auto-approval does. If an update in that window exposed it, this one withdraws it.
        let withdraws_visible_requests = self.tasks_with_visible_requests.contains(&task_id);
        if !pending_requests.is_empty() || clears_transient_requests || withdraws_visible_requests {
            // Permission/question opening precedes the Task mutation that marks
            // it waiting, while Task termination closes requests before its durable
            // status update is published. Project both edges through the same ordered
            // Task stream so live clients cannot retain stale request controls.
            self.record_published_task_requests(&task_id, &pending_requests, "task_update");
            events.extend(self.publish_task_payload(
                AppServerEventPayload::TaskRequestsUpdated {
                    task_id: task_id.clone(),
                    requests: pending_requests,
                },
                now,
            ));
        }
        // Task updates come from the publisher thread, never from a request, so they join the
        // outbox here; the returned copy lets callers inspect what was published.
        self.outbox.push_events(events.clone());
        events
    }

    fn publish_history_sync(
        &mut self,
        task_id: &TaskId,
        history_sync: openaide_app_server_protocol::snapshot::TaskHistorySyncSnapshot,
        now: AppServerTime,
    ) -> Vec<GatewayEventDelivery> {
        self.publish_task_payload(
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
            AppServerEventPayload::TaskChanged {
                task_id: task_id.clone(),
                revision,
                changes: change.changes.clone(),
            },
            now,
        );
        if change.changes.removed {
            // Subscriptions to the Task's Subagent catalog, Subagent Histories and Tool details
            // end with the Task.
            events.extend(self.publish_task_payload(
                AppServerEventPayload::TaskContentRemoved {
                    task_id: task_id.clone(),
                },
                now,
            ));
        }

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
            events.extend(self.publish_task_payload(
                AppServerEventPayload::ToolDetailChanged {
                    task_id: task_id.clone(),
                    artifact_id: detail.artifact_id.clone(),
                    revision: detail.details.revision,
                    deltas,
                },
                now,
            ));
        }

        events
    }

    /// Publishes a payload a Task owns to the subscriptions its payload kind addresses.
    pub(crate) fn publish_task_payload(
        &mut self,
        payload: AppServerEventPayload,
        now: AppServerTime,
    ) -> Vec<GatewayEventDelivery> {
        let Some(address) = EventAddress::of_task_payload(&payload) else {
            crate::logging::warn(
                "task_payload_without_task_address",
                serde_json::json!({ "operation": "publish_task_payload", "outcome": "dropped" }),
            );
            return Vec::new();
        };
        let client_hub = self.client_hub.clone();
        event_deliveries(self.state_stream.publish(
            address,
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
        self.publish_task_payload(
            AppServerEventPayload::ToolDetailChanged {
                task_id: task_id.clone(),
                artifact_id: artifact_id.to_string(),
                revision,
                deltas,
            },
            now,
        )
    }
}
