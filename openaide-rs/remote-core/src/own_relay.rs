//! An Own Relay: a relay its operator runs and admits people to by password
//! (ADR-0085). The operator hands each person one line, the relay's address with
//! their password, such as `https://relay.example.com/?token=<password>`.

use std::fmt;
use std::sync::Arc;

use iroh::{RelayConfig, RelayMap, RelayUrl};

/// A relay address and the password it admits this computer and its devices by.
///
/// The password is a secret. `Debug` hides it, and logs name only the host.
#[derive(Clone, PartialEq, Eq)]
pub struct OwnRelay {
    url: RelayUrl,
    token: String,
}

/// Why a text is not an Own Relay line. Each end words the message for its users.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum OwnRelayError {
    #[error("this is not a relay address")]
    NotAnAddress,
    #[error("a relay address starts with https://")]
    NotHttps,
    #[error("the relay address has no password")]
    NoPassword,
}

impl OwnRelay {
    /// Reads the line an operator hands out. Surrounding spaces are ignored. The
    /// address must use `https`, because the line carries a password.
    pub fn parse(line: &str) -> Result<Self, OwnRelayError> {
        let parsed: RelayUrl = line
            .trim()
            .parse()
            .map_err(|_| OwnRelayError::NotAnAddress)?;
        if parsed.scheme() != "https" {
            return Err(OwnRelayError::NotHttps);
        }
        if parsed.host_str().is_none_or(str::is_empty) {
            return Err(OwnRelayError::NotAnAddress);
        }
        let token = parsed
            .query_pairs()
            .find(|(name, _)| name == "token")
            .map(|(_, value)| value.into_owned())
            .filter(|token| !token.is_empty())
            .ok_or(OwnRelayError::NoPassword)?;
        let mut url = (*parsed).clone();
        url.set_query(None);
        url.set_fragment(None);
        Ok(Self {
            url: RelayUrl::from(url),
            token,
        })
    }

    /// The line this relay was read from, password included. Only the store and an
    /// invite carry it.
    pub fn line(&self) -> String {
        let mut url = (*self.url).clone();
        url.query_pairs_mut().append_pair("token", &self.token);
        url.to_string()
    }

    pub fn url(&self) -> &RelayUrl {
        &self.url
    }

    /// The relay's host name, safe to show and to log.
    pub fn host(&self) -> &str {
        self.url.host_str().unwrap_or_default()
    }

    /// The relay with its password, as an endpoint's relay map holds it. iroh sends
    /// the password only to the relay it belongs to.
    pub fn config(&self) -> Arc<RelayConfig> {
        Arc::new(RelayConfig::from(self.url.clone()).with_auth_token(self.token.clone()))
    }

    pub fn relay_map(&self) -> RelayMap {
        RelayMap::from(self.url.clone()).with_auth_token(self.token.clone())
    }

    /// The password alone, for the relay's management API, which takes it as a
    /// bearer token. Never log it.
    pub fn password(&self) -> &str {
        &self.token
    }
}

impl fmt::Debug for OwnRelay {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OwnRelay")
            .field("host", &self.host())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
#[path = "own_relay_tests.rs"]
mod tests;
