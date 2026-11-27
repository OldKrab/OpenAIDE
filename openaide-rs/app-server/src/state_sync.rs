use openaide_app_server_protocol::events::{AppServerEvent, AppServerEventPayload, EventScope};
use openaide_app_server_protocol::ids::{ClientInstanceId, EventCursor, StateRootId, TaskId};
use openaide_app_server_protocol::state::{
    StateSubscribeResult, StateUnsubscribeResult, SubscriptionScope,
};

use crate::client_lifecycle::{AppServerTime, ClientContext, Delivery};
use crate::snapshots::SnapshotProvider;
use crate::storage_runtime::{CursorSequencer, SnapshotReadToken};

/// One client's subscription and the cursor of the last event it was sent, or of its baseline.
/// Each record links its own chain, so a client's chain never skips an event that was meant for
/// another client, and a cursor gap on the client means App Server lost an event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionRecord {
    pub client_instance_id: ClientInstanceId,
    pub scope: SubscriptionScope,
    cursor: EventCursor,
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

#[derive(Debug, Clone)]
pub struct StateStream {
    state_root_id: StateRootId,
    /// Advances once per publication. Subscription records take their cursor values from it, so
    /// every value is unique and a baseline is ordered against every event.
    cursors: CursorSequencer,
    subscriptions: Vec<SubscriptionRecord>,
}

impl StateStream {
    pub fn new(state_root_id: StateRootId) -> Self {
        Self {
            state_root_id,
            cursors: CursorSequencer::new(),
            subscriptions: Vec::new(),
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
        known_version: Option<u64>,
        snapshot_provider: &impl SnapshotProvider,
        _now: AppServerTime,
    ) -> Result<(StateSubscribeResult, bool), openaide_app_server_protocol::errors::ProtocolError>
    {
        // TODO: Build the baseline outside the protocol lock if `chat_page_build_slow` shows it
        // stalling delivery on an optimized build. This needs the subscription registered first
        // and events held until the reply, because ADR-0023 pairs each baseline with its cursor.
        let token = self.cursors.read_token();
        let snapshot = snapshot_provider.snapshot(ctx, &scope, known_version, &token)?;
        let subscription_added = self.upsert_subscription(
            ctx.client_instance_id.clone(),
            scope.clone(),
            token.cursor().clone(),
        );
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
        self.subscriptions.retain(|subscription| {
            subscription.client_instance_id != ctx.client_instance_id || subscription.scope != scope
        });
        StateUnsubscribeResult { scope }
    }

    pub fn publish_committed(
        &mut self,
        scope: EventScope,
        payload: AppServerEventPayload,
        deliveries: impl Fn(&ClientInstanceId) -> Option<Delivery>,
        _now: AppServerTime,
    ) -> PublishOutcome {
        let (_, cursor) = self.cursors.advance();
        // A deleted Task ends its subscriptions: the event that reports the removal is the last
        // one they receive. The Task's own subscriptions end with `TaskChanged { removed }`,
        // its entity subscriptions with `TaskContentRemoved`.
        let removed_task = match (&scope, &payload) {
            (
                EventScope::Task { task_id, .. },
                AppServerEventPayload::TaskChanged { changes, .. },
            ) if changes.removed => Some((task_id.clone(), true)),
            (
                EventScope::Task { task_id, .. },
                AppServerEventPayload::TaskContentRemoved { .. },
            ) => Some((task_id.clone(), false)),
            _ => None,
        };
        let mut published = Vec::new();
        for subscription in self.subscriptions.iter_mut() {
            if !event_matches_subscription(&scope, &payload, subscription, &self.state_root_id) {
                continue;
            }
            // The record moves even when its client has no route, so a client that somehow
            // missed this event sees the gap on its next one.
            let previous_cursor = std::mem::replace(&mut subscription.cursor, cursor.clone());
            let Some(delivery) = deliveries(&subscription.client_instance_id) else {
                continue;
            };
            published.push(StateEventDelivery {
                delivery,
                event: AppServerEvent {
                    subscription: subscription.scope.clone(),
                    previous_cursor,
                    cursor: cursor.clone(),
                    scope: scope.clone(),
                    payload: payload.clone(),
                },
            });
        }
        if let Some((task_id, is_task_scope)) = removed_task {
            self.end_task_subscriptions(&task_id, is_task_scope);
        }
        PublishOutcome {
            deliveries: published,
        }
    }

    /// Drops a deleted Task's subscriptions: the Task's own when `task_scope`, else its entities'.
    fn end_task_subscriptions(&mut self, task_id: &TaskId, task_scope: bool) {
        let ends = |scope: &SubscriptionScope| match scope {
            SubscriptionScope::Task { task_id: t } => task_scope && t == task_id,
            SubscriptionScope::SubagentCatalog { task_id: t }
            | SubscriptionScope::SubagentHistory { task_id: t, .. }
            | SubscriptionScope::ToolDetail { task_id: t, .. } => !task_scope && t == task_id,
            _ => false,
        };
        self.subscriptions.retain(|s| !ends(&s.scope));
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

    /// Registers the subscription, or rebases an existing one, at its new baseline's cursor.
    /// Events for the old baseline that are already queued link to an older cursor, so the
    /// client recognizes and drops them. Returns whether the subscription is new.
    fn upsert_subscription(
        &mut self,
        client_instance_id: ClientInstanceId,
        scope: SubscriptionScope,
        baseline: EventCursor,
    ) -> bool {
        if let Some(existing) = self.subscriptions.iter_mut().find(|subscription| {
            subscription.client_instance_id == client_instance_id && subscription.scope == scope
        }) {
            existing.cursor = baseline;
            return false;
        }
        self.subscriptions.push(SubscriptionRecord {
            client_instance_id,
            scope,
            cursor: baseline,
        });
        true
    }
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
        ),
        // A deleted Task ends each of its entity subscriptions with this notification.
        SubscriptionScope::SubagentCatalog { task_id } => {
            matches!(
                payload,
                AppServerEventPayload::SubagentCatalogUpdated { catalog }
                    if &catalog.task_id == task_id
            ) || task_content_removed(payload, task_id)
        }
        SubscriptionScope::SubagentHistory {
            task_id,
            subagent_id,
        } => {
            matches!(
                payload,
                AppServerEventPayload::SubagentHistoryChanged {
                    task_id: changed_task_id,
                    subagent_id: changed_subagent_id,
                    ..
                } if changed_task_id == task_id && changed_subagent_id == subagent_id
            ) || task_content_removed(payload, task_id)
        }
        SubscriptionScope::ToolDetail {
            task_id,
            artifact_id,
        } => {
            matches!(
                payload,
                AppServerEventPayload::ToolDetailChanged {
                    task_id: updated_task_id,
                    artifact_id: updated_artifact_id,
                    ..
                } if updated_task_id == task_id && updated_artifact_id == artifact_id
            ) || task_content_removed(payload, task_id)
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

fn task_content_removed(payload: &AppServerEventPayload, task_id: &TaskId) -> bool {
    matches!(
        payload,
        AppServerEventPayload::TaskContentRemoved { task_id: removed } if removed == task_id
    )
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
