//! Who may relay: every connection must carry a relay key from the key store.
//!
//! [`RelayAccess`] also remembers which key each live connection used, so a revoked key's
//! connections can be cut without a restart. iroh's connection registry ([`Clients`])
//! belongs to the relay service, which is built after this access check, so it is attached
//! once the service exists.
//!
//! Revocation and admission race: a connection can pass the check just before its key is
//! revoked, or still be registering with iroh when the cut runs. [`RelayAccess::cut_revoked`]
//! therefore compares every live connection against the store rather than acting on one
//! revocation, and `serve` runs it on every key file check as well as after an API revoke.
//! A connection that slips through is cut on the next check.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use iroh_base::EndpointId;
use iroh_relay::server::clients::Clients;
use iroh_relay::server::{Access, AccessControl, ClientRequest, ConnectionId};
use serde_json::json;

use crate::keys::{KeyStore, TokenDigest};
use crate::log;

#[derive(Debug)]
struct LiveConnection {
    key: TokenDigest,
    since: Instant,
    /// Set once a cut has been asked of iroh, so it is reported only once.
    cut: bool,
}

#[derive(Debug)]
pub struct RelayAccess {
    store: Arc<KeyStore>,
    live: Mutex<HashMap<(EndpointId, ConnectionId), LiveConnection>>,
    clients: OnceLock<Clients>,
}

impl RelayAccess {
    pub fn new(store: Arc<KeyStore>) -> Self {
        Self {
            store,
            live: Mutex::new(HashMap::new()),
            clients: OnceLock::new(),
        }
    }

    /// Gives this access check the relay service's connection registry.
    pub fn attach(&self, clients: Clients) {
        let _ = self.clients.set(clients);
    }

    /// The number of live connections made with `token`.
    pub fn live_connections(&self, token: &str) -> usize {
        let key = TokenDigest::of(token);
        self.lock_live()
            .values()
            .filter(|connection| connection.key == key)
            .count()
    }

    /// Cuts every live connection whose key is no longer in the store. Returns how many
    /// cuts it asked iroh for.
    pub fn cut_revoked(&self) -> usize {
        let Some(clients) = self.clients.get() else {
            return 0;
        };
        let valid: HashSet<TokenDigest> = match self.store.people() {
            Ok(people) => people.iter().map(|person| person.token_digest()).collect(),
            Err(error) => {
                // A key file that cannot be read cuts nobody: the connections were
                // admitted by a key that was valid, and the next check retries.
                log::event(
                    "relay.cut_revoked",
                    json!({ "outcome": "error", "errorClass": error.class() }),
                );
                return 0;
            }
        };
        let mut cut = 0;
        for ((endpoint, connection_id), connection) in self.lock_live().iter_mut() {
            if valid.contains(&connection.key) {
                continue;
            }
            // `false` means iroh has not registered the connection yet; the next check
            // tries again.
            if clients.disconnect(*endpoint, Some(*connection_id)) && !connection.cut {
                connection.cut = true;
                cut += 1;
                log::event(
                    "relay.cut_revoked",
                    json!({
                        "outcome": "ok",
                        "keyId": connection.key.key_id(),
                        "endpoint": endpoint.fmt_short().to_string(),
                        "connectionId": connection_id.to_string(),
                    }),
                );
            }
        }
        cut
    }

    fn lock_live(
        &self,
    ) -> std::sync::MutexGuard<'_, HashMap<(EndpointId, ConnectionId), LiveConnection>> {
        self.live
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    fn deny(request: &ClientRequest, reason: &'static str, key: Option<&str>) -> Access {
        log::event(
            "relay.connect",
            json!({
                "outcome": "denied",
                "reason": reason,
                "keyId": key.map(log::key_id),
                "endpoint": request.endpoint_id().fmt_short().to_string(),
                "connectionId": request.connection_id().to_string(),
            }),
        );
        Access::Deny {
            reason: Some("this relay key is not valid here".to_string()),
        }
    }
}

impl AccessControl for RelayAccess {
    async fn on_connect(&self, request: &ClientRequest) -> Access {
        let Some(token) = request.auth_token() else {
            return Self::deny(request, "no_key", None);
        };
        let person = match self.store.person_for_token(&token) {
            Ok(Some(person)) => person,
            Ok(None) => return Self::deny(request, "unknown_key", Some(&token)),
            Err(error) => return Self::deny(request, error.class(), Some(&token)),
        };
        self.lock_live().insert(
            (request.endpoint_id(), request.connection_id()),
            LiveConnection {
                key: person.token_digest(),
                since: Instant::now(),
                cut: false,
            },
        );
        log::event(
            "relay.connect",
            json!({
                "outcome": "allowed",
                "keyId": person.key_id(),
                "endpoint": request.endpoint_id().fmt_short().to_string(),
                "connectionId": request.connection_id().to_string(),
            }),
        );
        Access::Allow
    }

    fn on_disconnect(&self, endpoint: EndpointId, connection_id: ConnectionId) {
        let Some(connection) = self.lock_live().remove(&(endpoint, connection_id)) else {
            return;
        };
        log::event(
            "relay.disconnect",
            json!({
                "keyId": connection.key.key_id(),
                "endpoint": endpoint.fmt_short().to_string(),
                "connectionId": connection_id.to_string(),
                "cut": connection.cut,
                "durationMs": log::millis_since(connection.since),
            }),
        );
    }
}
