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
    pub event: AppServerEvent,
    pub deliveries: Vec<Delivery>,
}

#[derive(Debug, Clone)]
pub struct StateStream {
    state_root_id: StateRootId,
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
        _now: AppServerTime,
    ) -> Result<StateSubscribeResult, openaide_app_server_protocol::errors::ProtocolError> {
        let token = self.cursors.read_token();
        self.upsert_subscription(ctx.client_instance_id.clone(), scope.clone());
        let snapshot = snapshot_provider.snapshot(ctx, &scope, &token)?;
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
        let (previous_cursor, cursor) = self.cursors.advance();
        let event = AppServerEvent {
            previous_cursor,
            cursor,
            scope: scope.clone(),
            payload,
        };
        let deliveries = self
            .subscribers_for_event(&scope, &event.payload)
            .into_iter()
            .filter_map(|client_id| deliveries(&client_id))
            .collect();
        PublishOutcome { event, deliveries }
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

    pub fn read_token(&self) -> SnapshotReadToken {
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

    fn subscribers_for_event(
        &self,
        scope: &EventScope,
        payload: &AppServerEventPayload,
    ) -> Vec<ClientInstanceId> {
        self.subscriptions
            .iter()
            .filter(|subscription| {
                event_matches_subscription(scope, payload, subscription, &self.state_root_id)
            })
            .map(|subscription| subscription.client_instance_id.clone())
            .collect()
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
                && matches!(subscription_scope, SubscriptionScope::Task { task_id: subscribed } if subscribed == task_id)
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
        SubscriptionScope::Projects => {
            matches!(
                payload,
                AppServerEventPayload::SnapshotReplaced { .. }
                    | AppServerEventPayload::ProjectCollectionUpdated { .. }
            )
        }
        SubscriptionScope::Agents => {
            matches!(
                payload,
                AppServerEventPayload::SnapshotReplaced { .. }
                    | AppServerEventPayload::ProjectCollectionUpdated { .. }
                    | AppServerEventPayload::AgentCollectionUpdated { .. }
            )
        }
        SubscriptionScope::Settings { .. } => {
            matches!(
                payload,
                AppServerEventPayload::SnapshotReplaced { .. }
                    | AppServerEventPayload::ProjectCollectionUpdated { .. }
            )
        }
        SubscriptionScope::TaskNavigation { project_id } => {
            if project_id.is_some()
                && matches!(payload, AppServerEventPayload::TaskNavigationUpdated { .. })
            {
                return false;
            }
            matches!(
                payload,
                AppServerEventPayload::SnapshotReplaced { .. }
                    | AppServerEventPayload::TaskNavigationUpdated { .. }
                    | AppServerEventPayload::ProjectCollectionUpdated { .. }
                    | AppServerEventPayload::TaskUpdated { .. }
            )
        }
        SubscriptionScope::Task { .. } => matches!(
            payload,
            AppServerEventPayload::SnapshotReplaced { .. }
                | AppServerEventPayload::ProjectCollectionUpdated { .. }
                | AppServerEventPayload::TaskUpdated { .. }
                | AppServerEventPayload::TaskSnapshotUpdated { .. }
                | AppServerEventPayload::ChatItemAppended { .. }
                | AppServerEventPayload::ChatItemChunk { .. }
                | AppServerEventPayload::RequestUpdated { .. }
        ),
    }
}

#[cfg(test)]
mod tests;
