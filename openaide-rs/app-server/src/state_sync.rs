use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use openaide_app_server_protocol::events::{AppServerEvent, AppServerEventPayload, EventScope};
use openaide_app_server_protocol::ids::{ClientInstanceId, StateRootId, SubagentId, TaskId};
use openaide_app_server_protocol::state::{
    StateSubscribeResult, StateUnsubscribeResult, SubscriptionScope, SubscriptionSnapshot,
    TaskResumeSnapshot,
};
use serde_json::json;

use crate::client_lifecycle::{AppServerTime, ClientContext, Delivery};
use crate::logging;
use crate::snapshots::SnapshotProvider;
use crate::storage_runtime::{CursorSequencer, SnapshotReadToken};

mod task_change_log;

use task_change_log::{ChangesSince, TaskChangeLog};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionRecord {
    pub client_instance_id: ClientInstanceId,
    pub scope: SubscriptionScope,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishOutcome {
    pub deliveries: Vec<StateEventDelivery>,
}

/// One client transport delivery with a cursor link scoped to that client's observed stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateEventDelivery {
    pub delivery: Delivery,
    pub event: AppServerEvent,
}

/// Shares current Task-view presence with App Server workflows without exposing
/// transport or client-subscription details to product logic.
#[derive(Debug, Clone, Default)]
pub(crate) struct TaskSubscriptionPresence {
    subscribers: Arc<Mutex<HashMap<TaskId, HashSet<ClientInstanceId>>>>,
    /// Clients watching one Subagent history. A history subscriber is registered
    /// before its baseline is read, so a change skipped because nobody was watching
    /// committed before that read and is part of the baseline.
    subagent_history_subscribers:
        Arc<Mutex<HashMap<SubagentHistoryKey, HashSet<ClientInstanceId>>>>,
}

type SubagentHistoryKey = (TaskId, SubagentId);

impl TaskSubscriptionPresence {
    fn observe_subscribed(&self, client_instance_id: ClientInstanceId, task_id: TaskId) {
        self.subscribers
            .lock()
            .expect("Task subscription presence poisoned")
            .entry(task_id)
            .or_default()
            .insert(client_instance_id);
    }

    fn observe_unsubscribed(&self, client_instance_id: &ClientInstanceId, task_id: &TaskId) {
        let mut subscribers = self
            .subscribers
            .lock()
            .expect("Task subscription presence poisoned");
        if let Some(task_subscribers) = subscribers.get_mut(task_id) {
            task_subscribers.remove(client_instance_id);
            if task_subscribers.is_empty() {
                subscribers.remove(task_id);
            }
        }
    }

    fn observe_client_expired(&self, client_instance_id: &ClientInstanceId) {
        self.subscribers
            .lock()
            .expect("Task subscription presence poisoned")
            .retain(|_, subscribers| {
                subscribers.remove(client_instance_id);
                !subscribers.is_empty()
            });
        self.subagent_history_subscribers
            .lock()
            .expect("Subagent history presence poisoned")
            .retain(|_, subscribers| {
                subscribers.remove(client_instance_id);
                !subscribers.is_empty()
            });
    }

    fn observe_subagent_history_subscribed(
        &self,
        client_instance_id: ClientInstanceId,
        task_id: TaskId,
        subagent_id: SubagentId,
    ) {
        self.subagent_history_subscribers
            .lock()
            .expect("Subagent history presence poisoned")
            .entry((task_id, subagent_id))
            .or_default()
            .insert(client_instance_id);
    }

    fn observe_subagent_history_unsubscribed(
        &self,
        client_instance_id: &ClientInstanceId,
        task_id: &TaskId,
        subagent_id: &SubagentId,
    ) {
        let mut subscribers = self
            .subagent_history_subscribers
            .lock()
            .expect("Subagent history presence poisoned");
        let key = (task_id.clone(), subagent_id.clone());
        if let Some(history_subscribers) = subscribers.get_mut(&key) {
            history_subscribers.remove(client_instance_id);
            if history_subscribers.is_empty() {
                subscribers.remove(&key);
            }
        }
    }

    /// Whether any client watches this Subagent history, so a change must be sent.
    pub(crate) fn watches_subagent_history(&self, task_id: &str, subagent_id: &str) -> bool {
        self.subagent_history_subscribers
            .lock()
            .expect("Subagent history presence poisoned")
            .keys()
            .any(|(task, subagent)| task.as_str() == task_id && subagent.as_str() == subagent_id)
    }

    pub(crate) fn has_subscribers(&self, task_id: &str) -> bool {
        self.subscribers
            .lock()
            .expect("Task subscription presence poisoned")
            .iter()
            .any(|(candidate, subscribers)| {
                candidate.as_str() == task_id && !subscribers.is_empty()
            })
    }

    #[cfg(test)]
    pub(crate) fn subscribe_for_test(&self, client_instance_id: ClientInstanceId, task_id: TaskId) {
        self.observe_subscribed(client_instance_id, task_id);
    }

    #[cfg(test)]
    pub(crate) fn watch_subagent_history_for_test(&self, task_id: &str, subagent_id: &str) {
        self.observe_subagent_history_subscribed(
            ClientInstanceId::from("client-watching-subagent"),
            TaskId::from(task_id),
            SubagentId::from(subagent_id),
        );
    }
}

#[derive(Debug, Clone)]
pub struct StateStream {
    state_root_id: StateRootId,
    // Client snapshots retain a coarse process cursor; live subscriptions use independent cursors.
    cursors: CursorSequencer,
    scope_cursors: HashMap<SubscriptionScope, CursorSequencer>,
    subscriptions: Vec<SubscriptionRecord>,
    task_subscription_presence: TaskSubscriptionPresence,
    task_changes: TaskChangeLog,
}

impl StateStream {
    pub fn new(state_root_id: StateRootId) -> Self {
        Self::with_task_subscription_presence(state_root_id, TaskSubscriptionPresence::default())
    }

    pub(crate) fn with_task_subscription_presence(
        state_root_id: StateRootId,
        task_subscription_presence: TaskSubscriptionPresence,
    ) -> Self {
        Self {
            state_root_id,
            cursors: CursorSequencer::new(),
            scope_cursors: HashMap::new(),
            subscriptions: Vec::new(),
            task_subscription_presence,
            task_changes: TaskChangeLog::default(),
        }
    }

    pub fn subscribe(
        &mut self,
        ctx: &ClientContext,
        scope: SubscriptionScope,
        snapshot_provider: &impl SnapshotProvider,
        now: AppServerTime,
    ) -> Result<StateSubscribeResult, openaide_app_server_protocol::errors::ProtocolError> {
        self.subscribe_with_membership(ctx, scope, None, snapshot_provider, now)
            .map(|(result, _)| result)
    }

    /// Returns whether this client actually gained the scope, allowing product work to
    /// distinguish first subscription from a baseline resync of existing membership.
    pub(crate) fn subscribe_with_membership(
        &mut self,
        ctx: &ClientContext,
        scope: SubscriptionScope,
        known_revision: Option<u64>,
        snapshot_provider: &impl SnapshotProvider,
        _now: AppServerTime,
    ) -> Result<(StateSubscribeResult, bool), openaide_app_server_protocol::errors::ProtocolError>
    {
        let token = self.read_token_for_scope(&scope);
        if let SubscriptionScope::SubagentHistory {
            task_id,
            subagent_id,
        } = &scope
        {
            self.task_subscription_presence
                .observe_subagent_history_subscribed(
                    ctx.client_instance_id.clone(),
                    task_id.clone(),
                    subagent_id.clone(),
                );
        }
        let snapshot = match &scope {
            SubscriptionScope::Task { task_id } => {
                self.task_snapshot(ctx, task_id, known_revision, snapshot_provider, &token)
            }
            _ => snapshot_provider.snapshot(ctx, &scope, &token),
        };
        let snapshot = match snapshot {
            Ok(snapshot) => snapshot,
            Err(error) => {
                if !self.has_subscription(&ctx.client_instance_id, &scope) {
                    self.observe_scope_left(&ctx.client_instance_id, &scope);
                }
                return Err(error);
            }
        };
        let subscription_added =
            self.upsert_subscription(ctx.client_instance_id.clone(), scope.clone());
        if subscription_added {
            if let SubscriptionScope::Task { task_id } = &scope {
                self.task_subscription_presence
                    .observe_subscribed(ctx.client_instance_id.clone(), task_id.clone());
            }
        }
        Ok((
            StateSubscribeResult {
                cursor: token.cursor().clone(),
                scope,
                snapshot,
            },
            subscription_added,
        ))
    }

    pub fn unsubscribe(
        &mut self,
        ctx: &ClientContext,
        scope: SubscriptionScope,
        _now: AppServerTime,
    ) -> StateUnsubscribeResult {
        let removed = self.subscriptions.len();
        self.subscriptions.retain(|subscription| {
            subscription.client_instance_id != ctx.client_instance_id || subscription.scope != scope
        });
        if self.subscriptions.len() != removed {
            self.observe_scope_left(&ctx.client_instance_id, &scope);
        }
        StateUnsubscribeResult { scope }
    }

    fn has_subscription(
        &self,
        client_instance_id: &ClientInstanceId,
        scope: &SubscriptionScope,
    ) -> bool {
        self.subscriptions.iter().any(|subscription| {
            &subscription.client_instance_id == client_instance_id && &subscription.scope == scope
        })
    }

    fn observe_scope_left(&self, client_instance_id: &ClientInstanceId, scope: &SubscriptionScope) {
        match scope {
            SubscriptionScope::Task { task_id } => self
                .task_subscription_presence
                .observe_unsubscribed(client_instance_id, task_id),
            SubscriptionScope::SubagentHistory {
                task_id,
                subagent_id,
            } => self
                .task_subscription_presence
                .observe_subagent_history_unsubscribed(client_instance_id, task_id, subagent_id),
            _ => {}
        }
    }

    pub fn publish_committed(
        &mut self,
        scope: EventScope,
        payload: AppServerEventPayload,
        deliveries: impl Fn(&ClientInstanceId) -> Option<Delivery>,
        _now: AppServerTime,
    ) -> PublishOutcome {
        self.cursors.advance();
        if let (
            EventScope::Task { task_id, .. },
            AppServerEventPayload::TaskChanged {
                revision, changes, ..
            },
        ) = (&scope, &payload)
        {
            // Every Task change passes here, so the log sees the exact published stream.
            self.task_changes.record(task_id, *revision, changes);
        }
        let matching = self
            .subscriptions
            .iter()
            .filter(|subscription| {
                event_matches_subscription(&scope, &payload, subscription, &self.state_root_id)
            })
            .cloned()
            .collect::<Vec<_>>();
        let mut by_scope = Vec::<(SubscriptionScope, Vec<ClientInstanceId>)>::new();
        for subscription in matching {
            if let Some((_, clients)) = by_scope
                .iter_mut()
                .find(|(candidate, _)| candidate == &subscription.scope)
            {
                if !clients.contains(&subscription.client_instance_id) {
                    clients.push(subscription.client_instance_id);
                }
            } else {
                by_scope.push((subscription.scope, vec![subscription.client_instance_id]));
            }
        }

        let mut published = Vec::new();
        for (subscription, clients) in by_scope {
            let (previous_cursor, cursor) = self
                .scope_cursors
                .entry(subscription.clone())
                .or_default()
                .advance();
            for client_id in clients {
                let Some(delivery) = deliveries(&client_id) else {
                    continue;
                };
                published.push(StateEventDelivery {
                    delivery,
                    event: AppServerEvent {
                        subscription: subscription.clone(),
                        previous_cursor: previous_cursor.clone(),
                        cursor: cursor.clone(),
                        scope: scope.clone(),
                        payload: payload.clone(),
                    },
                });
            }
        }
        PublishOutcome {
            deliveries: published,
        }
    }

    pub fn subscribers_for_scope(&self, scope: &SubscriptionScope) -> Vec<ClientInstanceId> {
        self.subscriptions
            .iter()
            .filter(|subscription| &subscription.scope == scope)
            .map(|subscription| subscription.client_instance_id.clone())
            .collect()
    }

    pub fn subscriptions_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
    ) -> Vec<SubscriptionScope> {
        self.subscriptions
            .iter()
            .filter(|subscription| &subscription.client_instance_id == client_instance_id)
            .map(|subscription| subscription.scope.clone())
            .collect()
    }

    pub fn drop_client_subscriptions(&mut self, client_instance_id: &ClientInstanceId) {
        self.subscriptions
            .retain(|subscription| &subscription.client_instance_id != client_instance_id);
        self.task_subscription_presence
            .observe_client_expired(client_instance_id);
    }

    pub fn subscription_count(&self) -> usize {
        self.subscriptions.len()
    }

    pub fn subscription_count_for_kind(
        &self,
        matches: impl Fn(&SubscriptionScope) -> bool,
    ) -> usize {
        self.subscriptions
            .iter()
            .filter(|subscription| matches(&subscription.scope))
            .count()
    }

    pub fn read_token(&self) -> SnapshotReadToken {
        self.cursors.read_token()
    }

    /// Client snapshots use a coarse cursor only as an initialization marker.
    pub fn read_token_for_client(
        &mut self,
        _client_instance_id: &ClientInstanceId,
    ) -> SnapshotReadToken {
        self.cursors.read_token()
    }

    pub fn state_root_id(&self) -> &StateRootId {
        &self.state_root_id
    }

    fn upsert_subscription(
        &mut self,
        client_instance_id: ClientInstanceId,
        scope: SubscriptionScope,
    ) -> bool {
        if !self.subscriptions.iter().any(|subscription| {
            subscription.client_instance_id == client_instance_id && subscription.scope == scope
        }) {
            self.subscriptions.push(SubscriptionRecord {
                client_instance_id,
                scope,
            });
            return true;
        }
        false
    }

    /// Answers a Task subscription with a resume when the client's known revision can
    /// be brought current from retained changes, and with a baseline otherwise.
    fn task_snapshot(
        &self,
        ctx: &ClientContext,
        task_id: &TaskId,
        known_revision: Option<u64>,
        snapshot_provider: &impl SnapshotProvider,
        token: &SnapshotReadToken,
    ) -> Result<SubscriptionSnapshot, openaide_app_server_protocol::errors::ProtocolError> {
        let scope = SubscriptionScope::Task {
            task_id: task_id.clone(),
        };
        let Some(known_revision) = known_revision else {
            log_task_subscription(task_id, None, None, Err("noKnownRevision"));
            return snapshot_provider.snapshot(ctx, &scope, token);
        };
        let Some(basis) = snapshot_provider.task_resume_basis(ctx, task_id)? else {
            log_task_subscription(task_id, Some(known_revision), None, Err("unsupported"));
            return snapshot_provider.snapshot(ctx, &scope, token);
        };
        let missed = if known_revision > basis.revision {
            Err("knownRevisionAhead")
        } else if known_revision == basis.revision {
            Ok((Vec::new(), 0))
        } else {
            match self.task_changes.changes_since(task_id, known_revision) {
                ChangesSince::Changes { changes, bytes } => Ok((changes, bytes)),
                ChangesSince::Unknown => Err("changesNotRetained"),
                ChangesSince::Evicted => Err("changesEvicted"),
            }
        };
        let resolution = missed
            .as_ref()
            .map(|(changes, bytes)| (changes.len(), *bytes))
            .map_err(|reason| *reason);
        log_task_subscription(
            task_id,
            Some(known_revision),
            Some(basis.revision),
            resolution,
        );
        let Ok((changes, _)) = missed else {
            return snapshot_provider.snapshot(ctx, &scope, token);
        };
        Ok(SubscriptionSnapshot::TaskResume {
            resume: TaskResumeSnapshot {
                task_id: task_id.clone(),
                known_revision,
                changes,
                history_sync: basis.history_sync,
                pending_requests: Vec::new(),
                subagents: basis.subagents,
                workspace_available: basis.workspace_available,
            },
        })
    }

    fn read_token_for_scope(&mut self, scope: &SubscriptionScope) -> SnapshotReadToken {
        self.scope_cursors
            .entry(scope.clone())
            .or_default()
            .read_token()
    }
}

/// One event per Task subscription explains whether a switch paid for a baseline.
fn log_task_subscription(
    task_id: &TaskId,
    known_revision: Option<u64>,
    current_revision: Option<u64>,
    resolution: Result<(usize, usize), &'static str>,
) {
    let (outcome, baseline_reason, change_count, change_bytes) = match resolution {
        Ok((0, _)) => ("current", None, 0, 0),
        Ok((count, bytes)) => ("missedChanges", None, count, bytes),
        Err(reason) => ("baseline", Some(reason), 0, 0),
    };
    logging::info(
        "task_subscription_resolved",
        json!({
            "task_id": task_id.as_str(),
            "outcome": outcome,
            "baseline_reason": baseline_reason,
            "known_revision": known_revision,
            "current_revision": current_revision,
            "change_count": change_count,
            "change_bytes": change_bytes,
        }),
    );
}

fn event_matches_subscription(
    event_scope: &EventScope,
    payload: &AppServerEventPayload,
    subscription: &SubscriptionRecord,
    state_root_id: &StateRootId,
) -> bool {
    if matches!(
        payload,
        AppServerEventPayload::ProjectCollectionUpdated { .. }
    ) {
        return event_scope_state_root_matches(event_scope, state_root_id)
            && payload_matches_subscription(payload, &subscription.scope);
    }

    let subscription_scope = &subscription.scope;
    let scope_matches = match event_scope {
        EventScope::StateRoot {
            state_root_id: event_state_root,
        } => {
            event_state_root == state_root_id
                && !matches!(
                    subscription_scope,
                    SubscriptionScope::Task { .. }
                        | SubscriptionScope::SubagentCatalog { .. }
                        | SubscriptionScope::SubagentHistory { .. }
                )
        }
        EventScope::Client {
            state_root_id: event_state_root,
            client_instance_id,
        } => {
            event_state_root == state_root_id
                && client_instance_id == &subscription.client_instance_id
                && !matches!(
                    subscription_scope,
                    SubscriptionScope::Task { .. }
                        | SubscriptionScope::SubagentCatalog { .. }
                        | SubscriptionScope::SubagentHistory { .. }
                )
        }
        EventScope::Task {
            state_root_id: event_state_root,
            task_id,
        } => {
            event_state_root == state_root_id
                && matches!(
                    subscription_scope,
                    SubscriptionScope::Task { task_id: subscribed }
                        | SubscriptionScope::ToolDetail { task_id: subscribed, .. }
                        | SubscriptionScope::SubagentCatalog { task_id: subscribed }
                        | SubscriptionScope::SubagentHistory { task_id: subscribed, .. }
                        if subscribed == task_id
                )
        }
    };
    scope_matches && payload_matches_subscription(payload, subscription_scope)
}

fn event_scope_state_root_matches(event_scope: &EventScope, state_root_id: &StateRootId) -> bool {
    match event_scope {
        EventScope::StateRoot {
            state_root_id: event_state_root,
        }
        | EventScope::Client {
            state_root_id: event_state_root,
            ..
        }
        | EventScope::Task {
            state_root_id: event_state_root,
            ..
        } => event_state_root == state_root_id,
    }
}

fn payload_matches_subscription(
    payload: &AppServerEventPayload,
    subscription_scope: &SubscriptionScope,
) -> bool {
    match subscription_scope {
        SubscriptionScope::Projects => matches!(
            payload,
            AppServerEventPayload::SnapshotReplaced { .. }
                | AppServerEventPayload::ProjectCollectionUpdated { .. }
        ),
        SubscriptionScope::Agents => {
            matches!(
                payload,
                AppServerEventPayload::SnapshotReplaced { .. }
                    | AppServerEventPayload::AgentCollectionUpdated { .. }
            )
        }
        SubscriptionScope::Settings { .. } => {
            matches!(payload, AppServerEventPayload::SnapshotReplaced { .. })
        }
        SubscriptionScope::Devices => matches!(
            payload,
            AppServerEventPayload::SnapshotReplaced { .. }
                | AppServerEventPayload::DeviceCollectionUpdated { .. }
        ),
        SubscriptionScope::Terminals => matches!(
            payload,
            AppServerEventPayload::TerminalCollectionUpdated { .. }
                | AppServerEventPayload::SnapshotReplaced { .. }
        ),
        SubscriptionScope::TaskNavigation {
            section,
            project_ids,
        } => {
            matches!(payload, AppServerEventPayload::SnapshotReplaced { .. })
                || matches!(
                    payload,
                    AppServerEventPayload::NavigationReplaced { navigation }
                        if navigation.section == *section
                )
                || matches!(
                    payload, AppServerEventPayload::RefreshStateChanged { .. }
                        if matches!(
                            section,
                            openaide_app_server_protocol::task::TaskNavigationSection::Tasks
                        )
                )
                || matches!(
                    payload,
                    AppServerEventPayload::ProjectEntriesReplaced {
                        section: changed_section,
                        project_id,
                        ..
                    } if changed_section == section
                        && project_ids
                            .as_ref()
                            .is_none_or(|project_ids| project_ids.contains(project_id))
                )
                || matches!(
                    payload,
                    AppServerEventPayload::TaskUpdated {
                        project_id,
                        task,
                    } if project_ids
                        .as_ref()
                        .is_none_or(|project_ids| project_ids.contains(project_id))
                        && task_lifecycle_matches_navigation_section(task.lifecycle, *section)
                )
        }
        SubscriptionScope::Task { .. } => matches!(
            payload,
            AppServerEventPayload::SnapshotReplaced { .. }
                | AppServerEventPayload::TaskChanged { .. }
                | AppServerEventPayload::TaskHistorySyncUpdated { .. }
                | AppServerEventPayload::TaskRequestsUpdated { .. }
                | AppServerEventPayload::RequestUpdated { .. }
        ),
        SubscriptionScope::SubagentCatalog { task_id } => matches!(
            payload,
            AppServerEventPayload::SubagentCatalogUpdated { catalog }
                if &catalog.task_id == task_id
        ),
        SubscriptionScope::SubagentHistory {
            task_id,
            subagent_id,
        } => matches!(
            payload,
            AppServerEventPayload::SubagentHistoryUpdated { history }
                if &history.task_id == task_id && &history.subagent_id == subagent_id
        ),
        SubscriptionScope::ToolDetail {
            task_id,
            artifact_id,
        } => {
            matches!(
                payload,
                AppServerEventPayload::ToolDetailUpdated {
                    task_id: updated_task_id,
                    artifact_id: updated_artifact_id,
                    ..
                } if updated_task_id == task_id && updated_artifact_id == artifact_id
            ) || matches!(
                payload,
                AppServerEventPayload::ToolDetailChanged {
                    task_id: updated_task_id,
                    artifact_id: updated_artifact_id,
                    ..
                } if updated_task_id == task_id && updated_artifact_id == artifact_id
            )
        }
        SubscriptionScope::WorktreeRepository { repository_id } => {
            matches!(payload, AppServerEventPayload::SnapshotReplaced { .. })
                || matches!(
                    payload,
                    AppServerEventPayload::WorktreeRepositoryUpdated {
                        repository_id: updated_repository_id,
                        ..
                    } if updated_repository_id == repository_id
                )
        }
    }
}

fn task_lifecycle_matches_navigation_section(
    lifecycle: openaide_app_server_protocol::snapshot::TaskLifecycle,
    section: openaide_app_server_protocol::task::TaskNavigationSection,
) -> bool {
    matches!(
        (lifecycle, section),
        (
            openaide_app_server_protocol::snapshot::TaskLifecycle::Open,
            openaide_app_server_protocol::task::TaskNavigationSection::Tasks
        ) | (
            openaide_app_server_protocol::snapshot::TaskLifecycle::Archived,
            openaide_app_server_protocol::task::TaskNavigationSection::Archive
        )
    )
}

#[cfg(test)]
#[path = "state_sync_tests.rs"]
mod tests;
