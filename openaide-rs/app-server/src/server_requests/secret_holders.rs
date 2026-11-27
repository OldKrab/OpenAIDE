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
}

/// How a key lookup ended, for the routing log. Counts only; never key ids.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecretRouting {
    pub holder_count: usize,
    pub reachable: bool,
}

impl SecretHolders {
    pub(super) fn declare(
        &mut self,
        client: ClientInstanceId,
        keys: impl IntoIterator<Item = String>,
    ) {
        self.declarations += 1;
        self.by_client.insert(
            client,
            Holding {
                declared_at: self.declarations,
                keys: keys.into_iter().collect(),
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
        self.secret_holders.declare(client, keys);
    }

    /// The connected client to ask for `key`: the one that declared most recently among those
    /// holding it, with a live delivery. The routing counts say why none was found.
    pub fn secret_holder_for(&self, key: &str) -> (Option<Delivery>, SecretRouting) {
        let mut holders: Vec<(&ClientInstanceId, &Holding)> = self
            .secret_holders
            .by_client
            .iter()
            .filter(|(_, holding)| holding.keys.contains(key))
            .collect();
        let holder_count = holders.len();
        holders.sort_by_key(|(_, holding)| std::cmp::Reverse(holding.declared_at));
        let delivery = holders.into_iter().find_map(|(client, _)| {
            self.available_responders
                .get(client)
                .map(|responder| responder.delivery.clone())
        });
        let routing = SecretRouting {
            holder_count,
            reachable: delivery.is_some(),
        };
        (delivery, routing)
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
