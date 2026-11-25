//! The relay's address and certificate source, kept as `<state dir>/config.json`.
//!
//! `setup` writes the file and `serve` reads it at start. The address is also what every
//! relay key line carries, so the commands that print a line read it too.

use std::fmt;
use std::io;
use std::net::IpAddr;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::files;

pub const DEFAULT_STATE_DIR: &str = "/var/lib/openaide-relay";
pub const DEFAULT_HTTPS_PORT: u16 = 443;
/// iroh clients look for QUIC address discovery on this port of every relay, so the relay
/// serves it there.
pub const DEFAULT_QUIC_PORT: u16 = iroh_relay::defaults::DEFAULT_RELAY_QUIC_PORT;
pub const CONFIG_FILE: &str = "config.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayConfig {
    pub host: Host,
    pub port: u16,
    pub cert: CertSource,
    /// The UDP port for QUIC address discovery; `None` turns it off.
    pub quic_port: Option<u16>,
    /// An ACME directory other than Let's Encrypt production, for testing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acme_directory: Option<String>,
}

/// Where the relay's certificate comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum CertSource {
    /// The relay gets and renews its own certificate from Let's Encrypt.
    Acme,
    /// Something else keeps certificate files in `dir` renewed; the relay rereads them.
    Files { dir: PathBuf },
}

impl RelayConfig {
    pub fn path(state_dir: &Path) -> PathBuf {
        state_dir.join(CONFIG_FILE)
    }

    /// Reads the config. `Ok(None)` means `setup` has not run yet.
    pub fn load(state_dir: &Path) -> io::Result<Option<Self>> {
        match std::fs::read(Self::path(state_dir)) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub fn save(&self, state_dir: &Path) -> io::Result<()> {
        let _lock = files::DirLock::acquire(state_dir)?;
        let mut text = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        text.push(b'\n');
        files::write_private(&Self::path(state_dir), &text)
    }

    /// The line a person pastes into OpenAIDE: `https://<host>[:<port>]/?token=<token>`.
    pub fn key_line(&self, token: &str) -> String {
        key_line(&self.host, self.port, token)
    }
}

/// See [`RelayConfig::key_line`]. The port is left out when it is 443.
pub fn key_line(host: &Host, port: u16, token: &str) -> String {
    if port == DEFAULT_HTTPS_PORT {
        format!("https://{}/?token={token}", host.url_host())
    } else {
        format!("https://{}:{port}/?token={token}", host.url_host())
    }
}

/// The relay's public address: a DNS name or an IP address.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Host {
    /// A lowercase DNS name without a trailing dot.
    Name(String),
    Ip(IpAddr),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidHost;

impl fmt::Display for InvalidHost {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("not a DNS name or an IP address")
    }
}

impl std::error::Error for InvalidHost {}

impl Host {
    /// Reads an IP address (an IPv6 one may be in brackets) or a DNS name.
    pub fn parse(text: &str) -> Result<Self, InvalidHost> {
        let text = text.trim();
        let unbracketed = text
            .strip_prefix('[')
            .and_then(|inner| inner.strip_suffix(']'))
            .unwrap_or(text);
        if let Ok(ip) = unbracketed.parse::<IpAddr>() {
            return Ok(Self::Ip(ip));
        }
        Self::parse_name(text)
    }

    /// Reads a DNS name: dot-separated labels of letters, digits and inner hyphens.
    pub fn parse_name(text: &str) -> Result<Self, InvalidHost> {
        let name = text.trim().trim_end_matches('.').to_ascii_lowercase();
        let valid_label = |label: &str| {
            (1..=63).contains(&label.len())
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                && !label.starts_with('-')
                && !label.ends_with('-')
        };
        let all_digits = name
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'.');
        if name.is_empty() || name.len() > 253 || all_digits || !name.split('.').all(valid_label) {
            return Err(InvalidHost);
        }
        Ok(Self::Name(name))
    }

    /// The host as it appears in a URL: an IPv6 address goes in brackets.
    pub fn url_host(&self) -> String {
        match self {
            Self::Name(name) => name.clone(),
            Self::Ip(IpAddr::V4(ip)) => ip.to_string(),
            Self::Ip(IpAddr::V6(ip)) => format!("[{ip}]"),
        }
    }
}

impl fmt::Display for Host {
    /// The bare host: a name, or an address without brackets.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Name(name) => formatter.write_str(name),
            Self::Ip(ip) => write!(formatter, "{ip}"),
        }
    }
}

impl Serialize for Host {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Host {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Host::parse(&text).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
