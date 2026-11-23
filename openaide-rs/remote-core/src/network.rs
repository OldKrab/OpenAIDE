//! The relays and address lookup an endpoint binds with (ADR-0062). A device and
//! its App Server can only find each other when both use the same network.

use iroh::endpoint::{presets, Builder};
use iroh::{Endpoint, RelayMode};

/// How an endpoint finds its peers and is found by them.
///
/// The test variants exist only with the `test-network` feature. Code outside
/// tests therefore calls the methods below instead of matching on the variants.
#[derive(Clone)]
pub enum Network {
    /// The public relays and address lookup that the iroh project operates.
    Public,
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
            #[cfg(feature = "test-network")]
            Self::Local(lookup) => {
                Endpoint::builder(presets::Minimal).address_lookup(lookup.clone())
            }
            #[cfg(feature = "test-network")]
            Self::Relay(map) => Endpoint::builder(presets::Minimal)
                .relay_mode(RelayMode::Custom(map.clone()))
                .ca_tls_config(iroh::tls::CaTlsConfig::insecure_skip_verify()),
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
            Self::Public => None,
            #[cfg(feature = "test-network")]
            Self::Local(_) => None,
            #[cfg(feature = "test-network")]
            Self::Relay(_) => Some(iroh::tls::CaTlsConfig::insecure_skip_verify()),
        }
    }
}
