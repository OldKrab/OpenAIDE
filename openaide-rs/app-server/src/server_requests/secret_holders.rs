//! The secure-storage keys each connected client declared (ADR-0086: `secret/read` goes to a
//! client that holds the key, not to a Task's subscribers).

use std::collections::{HashMap, HashSet};

use openaide_app_server_protocol::ids::ClientInstanceId;

use super::broker::ServerRequestBroker;
use crate::client_lifecycle::Delivery;

#[derive(Debug, Clone, Default)]
pub(super) struct SecretHolders {
    /// Advances with every declaration, so the client that declared last is the preferred holder.
    declarations: u64,
    by_client: HashMap<ClientInstanceId, Holding>,
}

#[derive(Debug, Clone)]
struct Holding {
    declared_at: u64,
    keys: HashSet<String>,
    /// May hold keys it did not list; asked only when no client declared the key.
    unknown_holder: bool,
}

/// How a key lookup ended, for the routing log. Counts only; never key ids.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecretRouting {
    pub holder_count: usize,
    pub reachable: bool,
    /// The holder was an unknown-holder client, not one that declared the key.
    pub unknown_holder_fallback: bool,
}

impl SecretHolders {
    pub(super) fn declare(
        &mut self,
        client: ClientInstanceId,
        keys: impl IntoIterator<Item = String>,
        unknown_holder: bool,
    ) {
        self.declarations += 1;
        self.by_client.insert(
            client,
            Holding {
                declared_at: self.declarations,
                keys: keys.into_iter().collect(),
                unknown_holder,
            },
        );
    }

    pub(super) fn forget(&mut self, client: &ClientInstanceId) {
        self.by_client.remove(client);
    }
}

impl ServerRequestBroker {
    /// Replaces the keys `client` holds. A client that never declares holds none.
    pub fn declare_secret_keys(
        &mut self,
        client: ClientInstanceId,
        keys: impl IntoIterator<Item = String>,
    ) {
        self.secret_holders.declare(client, keys, false);
    }

    /// As `declare_secret_keys`, and marks whether the client may hold keys it cannot list.
    pub fn declare_secret_holding(
        &mut self,
        client: ClientInstanceId,
        keys: impl IntoIterator<Item = String>,
        unknown_holder: bool,
    ) {
        self.secret_holders.declare(client, keys, unknown_holder);
    }

    /// The connected client to ask for `key`: the one that declared most recently among those
    /// holding it, with a live delivery. Only when none can answer, the most recent connected
    /// unknown-holder client is asked instead (secrets stored before the client kept a key
    /// index). The routing counts say why none was found.
    pub fn secret_holder_for(&self, key: &str) -> (Option<Delivery>, SecretRouting) {
        let holders = self.live_secret_holders(|holding| holding.keys.contains(key));
        let holder_count = holders.len();
        if let Some(delivery) = holders.into_iter().next() {
            return (
                Some(delivery),
                SecretRouting {
                    holder_count,
                    reachable: true,
                    unknown_holder_fallback: false,
                },
            );
        }
        let fallback = self.live_secret_holders(|holding| holding.unknown_holder);
        let delivery = fallback.into_iter().next();
        let routing = SecretRouting {
            holder_count,
            reachable: delivery.is_some(),
            unknown_holder_fallback: delivery.is_some(),
        };
        (delivery, routing)
    }

    /// Live deliveries of the clients matching `holds`, most recent declaration first.
    fn live_secret_holders(&self, holds: impl Fn(&Holding) -> bool) -> Vec<Delivery> {
        let mut holders: Vec<(&ClientInstanceId, &Holding)> = self
            .secret_holders
            .by_client
            .iter()
            .filter(|(_, holding)| holds(holding))
            .collect();
        holders.sort_by_key(|(_, holding)| std::cmp::Reverse(holding.declared_at));
        holders
            .into_iter()
            .filter_map(|(client, _)| {
                self.available_responders
                    .get(client)
                    .map(|responder| responder.delivery.clone())
            })
            .collect()
    }
}

impl ServerRequestBroker {
    pub fn task_of_request(
        &self,
        request_id: &openaide_app_server_protocol::ids::RequestId,
    ) -> Option<openaide_app_server_protocol::ids::TaskId> {
        match &self.records.get(request_id)?.snapshot.scope {
            openaide_app_server_protocol::snapshot::PendingRequestScope::Task { task_id } => {
                Some(task_id.clone())
            }
            _ => None,
        }
    }
}
