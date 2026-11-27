use openaide_app_server_protocol::events::{AppServerEvent, AppServerEventPayload};
use openaide_app_server_protocol::ids::{ClientInstanceId, TaskId};
use openaide_app_server_protocol::state::{
    StateSubscribeResult, StateUnsubscribeResult, SubscriptionScope,
};

use crate::client_lifecycle::{AppServerTime, ClientContext, Delivery};
use crate::snapshots::SnapshotProvider;

/// One client's subscription to a scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionRecord {
    pub client_instance_id: ClientInstanceId,
    pub scope: SubscriptionScope,
}

/// Who a published event is for. Each payload names its audience explicitly; there is no
/// matching of payloads against subscriptions (ADR-0086).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventAddress {
    /// Every client subscribed to the scope.
    Subscribers(SubscriptionScope),
    /// One client's subscription, for a Navigation view's frame.
    Client {
        client: ClientInstanceId,
        scope: SubscriptionScope,
    },
    /// The Task is deleted: its own subscribers receive the event, and their subscriptions end.
    TaskRemoved(TaskId),
    /// The Task is deleted: its Subagent catalog, Subagent History and Tool detail subscriptions
    /// receive the event, and end.
    TaskContentRemoved(TaskId),
}

impl EventAddress {
    /// The audience of a payload a Task owns. `None` for a Navigation frame, whose audience is
    /// one client's view and which the Navigation publisher addresses itself.
    pub fn of_task_payload(payload: &AppServerEventPayload) -> Option<Self> {
        let scope = |scope| Some(Self::Subscribers(scope));
        match payload {
            AppServerEventPayload::TaskChanged {
                task_id, changes, ..
            } if changes.removed => Some(Self::TaskRemoved(task_id.clone())),
            AppServerEventPayload::TaskChanged { task_id, .. }
            | AppServerEventPayload::TaskHistorySyncUpdated { task_id, .. }
            | AppServerEventPayload::TaskRequestsUpdated { task_id, .. } => {
                scope(SubscriptionScope::Task {
                    task_id: task_id.clone(),
                })
            }
            AppServerEventPayload::SubagentCatalogUpdated { catalog } => {
                scope(SubscriptionScope::SubagentCatalog {
                    task_id: catalog.task_id.clone(),
                })
            }
            AppServerEventPayload::SubagentHistoryChanged {
                task_id,
                subagent_id,
                ..
            } => scope(SubscriptionScope::SubagentHistory {
                task_id: task_id.clone(),
                subagent_id: subagent_id.clone(),
            }),
            AppServerEventPayload::ToolDetailChanged {
                task_id,
                artifact_id,
                ..
            } => scope(SubscriptionScope::ToolDetail {
                task_id: task_id.clone(),
                artifact_id: artifact_id.clone(),
            }),
            AppServerEventPayload::TaskContentRemoved { task_id } => {
                Some(Self::TaskContentRemoved(task_id.clone()))
            }
            AppServerEventPayload::NavigationFrame { .. } => None,
        }
    }

    fn reaches(&self, record: &SubscriptionRecord) -> bool {
        match self {
            Self::Subscribers(scope) => &record.scope == scope,
            Self::Client { client, scope } => {
                &record.client_instance_id == client && &record.scope == scope
            }
            Self::TaskRemoved(task_id) => {
                matches!(&record.scope, SubscriptionScope::Task { task_id: t } if t == task_id)
            }
            Self::TaskContentRemoved(task_id) => is_task_content_of(&record.scope, task_id),
        }
    }

    /// Whether delivering to a subscription ends it.
    fn ends(&self) -> bool {
        matches!(self, Self::TaskRemoved(_) | Self::TaskContentRemoved(_))
    }
}

fn is_task_content_of(scope: &SubscriptionScope, task_id: &TaskId) -> bool {
    matches!(
        scope,
        SubscriptionScope::SubagentCatalog { task_id: t }
            | SubscriptionScope::SubagentHistory { task_id: t, .. }
            | SubscriptionScope::ToolDetail { task_id: t, .. } if t == task_id
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishOutcome {
    pub deliveries: Vec<StateEventDelivery>,
}

/// One event for one client's transport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateEventDelivery {
    pub delivery: Delivery,
    pub event: AppServerEvent,
}

#[derive(Debug, Clone, Default)]
pub struct StateStream {
    subscriptions: Vec<SubscriptionRecord>,
}

impl StateStream {
    pub fn new() -> Self {
        Self {
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
        // stalling delivery on an optimized build. The subscription would be registered first
        // and its events held until the reply.
        let snapshot = snapshot_provider.snapshot(ctx, &scope, known_version)?;
        let subscription_added =
            self.upsert_subscription(ctx.client_instance_id.clone(), scope.clone());
        Ok((StateSubscribeResult { scope, snapshot }, subscription_added))
    }

    /// Registers a view subscription. The caller computes the baseline frame itself, because a
    /// view's content is not an entity snapshot.
    pub(crate) fn register_view_subscription(
        &mut self,
        ctx: &ClientContext,
        scope: SubscriptionScope,
    ) {
        self.upsert_subscription(ctx.client_instance_id.clone(), scope);
    }

    pub(crate) fn is_subscribed(&self, ctx: &ClientContext, scope: &SubscriptionScope) -> bool {
        self.subscriptions.iter().any(|subscription| {
            subscription.client_instance_id == ctx.client_instance_id
                && &subscription.scope == scope
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

    /// Delivers one event to the subscriptions `address` names, in publish order. A removal's
    /// event is the last one its subscriptions receive: they end right after it.
    pub fn publish(
        &mut self,
        address: EventAddress,
        payload: AppServerEventPayload,
        deliveries: impl Fn(&ClientInstanceId) -> Option<Delivery>,
        _now: AppServerTime,
    ) -> PublishOutcome {
        let mut published = Vec::new();
        for subscription in self
            .subscriptions
            .iter()
            .filter(|subscription| address.reaches(subscription))
        {
            let Some(delivery) = deliveries(&subscription.client_instance_id) else {
                continue;
            };
            published.push(StateEventDelivery {
                delivery,
                event: AppServerEvent {
                    subscription: subscription.scope.clone(),
                    payload: payload.clone(),
                },
            });
        }
        if address.ends() {
            self.subscriptions
                .retain(|subscription| !address.reaches(subscription));
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

    /// Registers the subscription. Returns whether it is new.
    fn upsert_subscription(
        &mut self,
        client_instance_id: ClientInstanceId,
        scope: SubscriptionScope,
    ) -> bool {
        if self.subscriptions.iter().any(|subscription| {
            subscription.client_instance_id == client_instance_id && subscription.scope == scope
        }) {
            return false;
        }
        self.subscriptions.push(SubscriptionRecord {
            client_instance_id,
            scope,
        });
        true
    }
}

#[cfg(test)]
#[path = "state_sync_tests.rs"]
mod tests;
