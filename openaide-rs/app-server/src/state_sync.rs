use std::collections::HashMap;

use openaide_app_server_protocol::events::{AppServerEvent, AppServerEventPayload, EventScope};
use openaide_app_server_protocol::ids::{ClientInstanceId, StateRootId};
use openaide_app_server_protocol::state::{
    StateSubscribeResult, StateUnsubscribeResult, SubscriptionScope,
};

use crate::client_lifecycle::{AppServerTime, ClientContext, Delivery};
use crate::snapshots::SnapshotProvider;
use crate::storage_runtime::{CursorSequencer, SnapshotReadToken};

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

#[derive(Debug, Clone)]
pub struct StateStream {
    state_root_id: StateRootId,
    // Client snapshots retain a coarse process cursor; live subscriptions use independent cursors.
    cursors: CursorSequencer,
    scope_cursors: HashMap<SubscriptionScope, CursorSequencer>,
    subscriptions: Vec<SubscriptionRecord>,
}

impl StateStream {
    pub fn new(state_root_id: StateRootId) -> Self {
        Self {
            state_root_id,
            cursors: CursorSequencer::new(),
            scope_cursors: HashMap::new(),
            subscriptions: Vec::new(),
        }
    }

    pub fn subscribe(
        &mut self,
        ctx: &ClientContext,
        scope: SubscriptionScope,
        snapshot_provider: &impl SnapshotProvider,
        _now: AppServerTime,
    ) -> Result<StateSubscribeResult, openaide_app_server_protocol::errors::ProtocolError> {
        let token = self.read_token_for_scope(&scope);
        let snapshot = snapshot_provider.snapshot(ctx, &scope, &token)?;
        self.upsert_subscription(ctx.client_instance_id.clone(), scope.clone());
        Ok(StateSubscribeResult {
            cursor: token.cursor().clone(),
            scope,
            snapshot,
        })
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
        self.cursors.advance();
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
    ) {
        if !self.subscriptions.iter().any(|subscription| {
            subscription.client_instance_id == client_instance_id && subscription.scope == scope
        }) {
            self.subscriptions.push(SubscriptionRecord {
                client_instance_id,
                scope,
            });
        }
    }

    fn read_token_for_scope(&mut self, scope: &SubscriptionScope) -> SnapshotReadToken {
        self.scope_cursors
            .entry(scope.clone())
            .or_default()
            .read_token()
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
                && !matches!(subscription_scope, SubscriptionScope::Task { .. })
        }
        EventScope::Client {
            state_root_id: event_state_root,
            client_instance_id,
        } => {
            event_state_root == state_root_id
                && client_instance_id == &subscription.client_instance_id
                && !matches!(subscription_scope, SubscriptionScope::Task { .. })
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
        SubscriptionScope::TaskNavigation { project_id } => {
            matches!(payload, AppServerEventPayload::SnapshotReplaced { .. })
                || matches!(
                    payload,
                    AppServerEventPayload::TaskNavigationReplaced { .. }
                )
                || matches!(
                    payload,
                    AppServerEventPayload::TaskNavigationChanged {
                        change:
                            openaide_app_server_protocol::events::TaskNavigationChange::Remove { .. }
                    }
                )
                || matches!(
                    payload,
                    AppServerEventPayload::TaskNavigationChanged {
                        change: openaide_app_server_protocol::events::TaskNavigationChange::Upsert { task }
                    } if project_id.as_ref().is_none_or(|project_id| &task.project_id == project_id)
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

#[cfg(test)]
#[path = "state_sync_tests.rs"]
mod tests;
