//! The relays and address lookup an endpoint binds with (ADR-0062, ADR-0085). A
//! device and its App Server can only find each other when both use the same network.

use iroh::endpoint::{presets, Builder};
use iroh::{Endpoint, RelayMode};

use crate::OwnRelay;

/// How an endpoint finds its peers and is found by them.
///
/// The test variants exist only with the `test-network` feature. Code outside
/// tests therefore calls the methods below instead of matching on the variants.
#[derive(Clone)]
pub enum Network {
    /// The public relays and address lookup that the iroh project operates.
    Public,
    /// One relay its operator runs, reached with a password (ADR-0085). Nothing is
    /// published to or looked up in the public address lookup: a device learns the
    /// relay from its invite and dials the App Server there.
    Own(OwnRelay),
    /// Direct addresses shared in memory, so a test needs no network service.
    #[cfg(feature = "test-network")]
    Local(iroh::address_lookup::MemoryLookup),
    /// A relay that the test runs itself, trusted without a certificate check.
    #[cfg(feature = "test-network")]
    Relay(iroh::RelayMap),
}

impl Network {
    /// An endpoint builder for this network, before the key and protocols are set.
    pub fn builder(&self) -> Builder {
        match self {
            Self::Public => Endpoint::builder(presets::N0),
            Self::Own(relay) => {
                Endpoint::builder(presets::Minimal).relay_mode(RelayMode::Custom(relay.relay_map()))
            }
            #[cfg(feature = "test-network")]
            Self::Local(lookup) => {
                Endpoint::builder(presets::Minimal).address_lookup(lookup.clone())
            }
            // A copy: clones of an iroh relay map share one map, and an endpoint adds a
            // computer's Own Relay to its own. Each endpoint starts from the network's
            // relays alone, as it does on the public network.
            #[cfg(feature = "test-network")]
            Self::Relay(map) => Endpoint::builder(presets::Minimal)
                .relay_mode(RelayMode::Custom(map.relays()))
                .ca_tls_config(iroh::tls::CaTlsConfig::insecure_skip_verify()),
        }
    }

    /// This network with the App Server's Own Relay in place of its relays, when one
    /// is set. A test network keeps its own lookup or certificate settings.
    pub fn with_own_relay(&self, relay: Option<&OwnRelay>) -> Self {
        let Some(relay) = relay else {
            return self.clone();
        };
        match self {
            Self::Public | Self::Own(_) => Self::Own(relay.clone()),
            #[cfg(feature = "test-network")]
            Self::Local(lookup) => Self::Local(lookup.clone()),
            #[cfg(feature = "test-network")]
            Self::Relay(_) => Self::Relay(relay.relay_map()),
        }
    }

    /// Makes a bound endpoint findable on a network that has no lookup service.
    pub fn on_bound(&self, endpoint: &Endpoint) {
        #[cfg(feature = "test-network")]
        if let Self::Local(lookup) = self {
            lookup.add_endpoint_info(endpoint.addr());
        }
        let _ = endpoint;
    }

    /// The relays that endpoints on this network home on, or `None` if the network
    /// has no relays.
    pub fn relay_mode(&self) -> Option<RelayMode> {
        match self {
            Self::Public => Some(iroh::endpoint::default_relay_mode()),
            Self::Own(relay) => Some(RelayMode::Custom(relay.relay_map())),
            #[cfg(feature = "test-network")]
            Self::Local(_) => None,
            #[cfg(feature = "test-network")]
            Self::Relay(map) => Some(RelayMode::Custom(map.clone())),
        }
    }

    /// The TLS settings for reaching this network's relays, or `None` to use the
    /// default settings.
    pub fn relay_tls(&self) -> Option<iroh::tls::CaTlsConfig> {
        match self {
            Self::Public | Self::Own(_) => None,
            #[cfg(feature = "test-network")]
            Self::Local(_) => None,
            #[cfg(feature = "test-network")]
            Self::Relay(_) => Some(iroh::tls::CaTlsConfig::insecure_skip_verify()),
        }
    }
}
