use openaide_app_server_protocol::events::{AppServerEventPayload, EventScope};
use openaide_app_server_protocol::ids::{ProjectId, TaskId};
use openaide_app_server_protocol::snapshot::TaskStatus;
use openaide_app_server_protocol::task::{TaskNavigationSection, TaskNavigationSection::*};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::client_lifecycle::{AppServerTime, ConnectionId};
use crate::server_requests::ServerRequestDelivery;
use crate::snapshots::TaskNavigationSnapshotSource;
use crate::task_events::{
    CommittedNavigationChange, CommittedTaskChange, TaskUpdate, TaskUpdateKind,
};

use super::RpcGateway;
use crate::protocol_edge::{event_deliveries, GatewayEventDelivery};

/// A Project's navigation entries read ahead of the protocol lock. The projection reads the
/// whole Project's Tasks and Native Sessions, which must not stall client requests.
pub(crate) struct PreparedProjectEntries {
    /// Generation when the read began. A request can publish a fresher projection before this
    /// one reaches the lock, and publishing this one after it would move clients backwards.
    generation: u64,
    payloads: Vec<AppServerEventPayload>,
}

impl PreparedProjectEntries {
    /// Reads before the lock is safe because only the publisher thread calls this: its updates
    /// stay in order, and a commit after this read has its own invalidation queued behind it.
    pub(crate) fn for_update(
        update: &TaskUpdate,
        navigation: &dyn TaskNavigationSnapshotSource,
        generation: &AtomicU64,
    ) -> Option<Self> {
        let project_id = match &update.kind {
            TaskUpdateKind::NavigationProjectEntriesChanged { project_id } => project_id,
            TaskUpdateKind::Changed(change) => match &change.navigation {
                Some(CommittedNavigationChange::ProjectEntriesChanged { project_id }) => project_id,
                _ => return None,
            },
            _ => return None,
        };
        let generation = generation.load(Ordering::SeqCst);
        Some(Self {
            generation,
            payloads: project_entries_payloads(navigation, project_id),
        })
    }
}

fn project_entries_payloads(
    navigation: &dyn TaskNavigationSnapshotSource,
    project_id: &ProjectId,
) -> Vec<AppServerEventPayload> {
    let mut payloads = Vec::new();
    for section in [Tasks, Archive] {
        let selected = [project_id.clone()];
        let snapshot = match navigation.snapshot(section, Some(&selected)) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                crate::logging::warn(
                    "project_entries_projection_failed",
                    serde_json::json!({
                        "operation": "task_update_publication",
                        "project_id": project_id,
                        "section": format!("{section:?}"),
                        "error_code": format!("{:?}", error.code),
                    }),
                );
                continue;
            }
        };
        let group = snapshot.groups.into_iter().next();
        let (task_count, entries, has_more, loading) = group
            .map(|group| {
                (
                    group.task_count,
                    group.entries,
                    group.has_more,
                    group.loading,
                )
            })
            .unwrap_or((0, Vec::new(), false, false));
        payloads.push(AppServerEventPayload::ProjectEntriesReplaced {
            section,
            project_id: project_id.clone(),
            task_count,
            entries,
            has_more,
            loading,
        });
    }
    payloads
}

impl RpcGateway {
    pub(crate) fn publish_task_update(
        &mut self,
        update: &TaskUpdate,
        now: AppServerTime,
    ) -> Vec<GatewayEventDelivery> {
        self.publish_prepared_task_update(update, None, now)
    }

    pub(crate) fn publish_prepared_task_update(
        &mut self,
        update: &TaskUpdate,
        prepared: Option<PreparedProjectEntries>,
        now: AppServerTime,
    ) -> Vec<GatewayEventDelivery> {
        let task_id = TaskId::from(update.task_id.clone());
        let mut events = match &update.kind {
            TaskUpdateKind::NavigationProjectEntriesChanged { project_id } => {
                self.publish_entries_for_update(project_id, prepared, now)
            }
            TaskUpdateKind::NavigationRefreshStateChanged { refresh } => {
                self.publish_refresh_state_changed(refresh.clone(), now)
            }
            TaskUpdateKind::HistorySync(history_sync) => {
                self.publish_history_sync(&task_id, history_sync.clone(), now)
            }
            TaskUpdateKind::Changed(change) => {
                self.publish_committed_task_change(&task_id, update.revision, change, prepared, now)
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
                &task_id,
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
                &task_id,
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
        if !pending_requests.is_empty() || clears_transient_requests {
            // Permission/question opening precedes the Task mutation that marks
            // it waiting, while Task termination closes requests before its durable
            // status update is published. Project both edges through the same ordered
            // Task stream so live clients cannot retain stale request controls.
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
        // Replaces every Project's entries, so it overtakes a projection in flight too.
        self.project_entries_generation
            .fetch_add(1, Ordering::SeqCst);
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
        prepared: Option<PreparedProjectEntries>,
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
                    self.publish_entries_for_update(project_id, prepared, now)
                }
            });
        }
        events
    }

    pub(crate) fn publish_task_payload(
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

    /// Reads and publishes under the protocol lock, for request handlers that must answer with
    /// the result. Any projection read before this one is now stale.
    pub(crate) fn publish_project_entries_replaced(
        &mut self,
        project_id: &ProjectId,
        now: AppServerTime,
    ) -> Vec<GatewayEventDelivery> {
        self.project_entries_generation
            .fetch_add(1, Ordering::SeqCst);
        let navigation = self.snapshots.task_navigation_source();
        let payloads = project_entries_payloads(navigation.as_ref(), project_id);
        self.publish_project_entries(payloads, now)
    }

    fn publish_entries_for_update(
        &mut self,
        project_id: &ProjectId,
        prepared: Option<PreparedProjectEntries>,
        now: AppServerTime,
    ) -> Vec<GatewayEventDelivery> {
        let generation = self.project_entries_generation.load(Ordering::SeqCst);
        match prepared {
            Some(prepared) if prepared.generation == generation => {
                self.publish_project_entries(prepared.payloads, now)
            }
            stale => {
                if stale.is_some() {
                    crate::logging::info(
                        "project_entries_projection_superseded",
                        serde_json::json!({
                            "operation": "task_update_publication",
                            "project_id": project_id,
                        }),
                    );
                }
                self.publish_project_entries_replaced(project_id, now)
            }
        }
    }

    fn publish_project_entries(
        &mut self,
        payloads: Vec<AppServerEventPayload>,
        now: AppServerTime,
    ) -> Vec<GatewayEventDelivery> {
        let mut events = Vec::new();
        for payload in payloads {
            let client_hub = self.client_hub.clone();
            events.extend(event_deliveries(self.state_stream.publish_committed(
                EventScope::StateRoot {
                    state_root_id: self.state_stream.state_root_id().clone(),
                },
                payload,
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
