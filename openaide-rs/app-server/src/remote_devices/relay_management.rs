//! The Own Relay a computer is reached through, and the people a manage key can
//! add to it (ADR-0085).
//!
//! A pasted relay key is checked with the relay before it is kept. A key that can
//! manage is kept for managing only: the App Server adds an ordinary key named
//! after the computer and relays, and shows Pairing Codes, with that one. So a
//! device never receives a key that can add or revoke people.
//!
//! Every call to the relay happens without the Remote Devices state lock, and
//! changes are serialized by `relay_ops`, so two clients changing the relay at
//! once cannot interleave.

use std::sync::MutexGuard;

use iroh::PublicKey;
use openaide_app_server_protocol::devices::{
    DevicesRelayAddPersonResult, DevicesRelayPeopleResult, DevicesSetRelayResult, OwnRelayAccess,
    RelayPerson,
};
use openaide_app_server_protocol::errors::{ErrorTarget, ProtocolError, ProtocolErrorCode};
use openaide_remote_core::OwnRelay;

use super::relay_api::{Person, RelayApiError};
use super::store::RelayKeys;
use super::{notify, storage_error, EdgeCommand, RemoteDevices};
use crate::logging;

/// The relay limits names to this many characters; a suffix may follow the computer's.
const RELAY_NAME_CHARS: usize = 40;
/// Names tried for this computer's own key before giving up: "desk", "desk 2", …
const OWN_NAME_ATTEMPTS: usize = 5;

/// Which call a relay failure came from, to word its message.
#[derive(Clone, Copy)]
enum Asked {
    /// Checking a pasted key.
    Key,
    /// Managing people with the kept manage key.
    Manage,
}

impl RemoteDevices {
    /// Moves the App Server onto the relay `line` names, or back to the public relays.
    pub(super) fn change_relay(
        &self,
        line: Option<&str>,
    ) -> Result<DevicesSetRelayResult, ProtocolError> {
        let pasted = line.map(parse_key).transpose()?;
        let _serial = self.relay_ops();
        let current = self.state().stored.relay_keys();
        let unchanged = match &pasted {
            None => current.relay.is_none(),
            Some(key) => {
                current.relay.as_ref() == Some(key) || current.manager.as_ref() == Some(key)
            }
        };
        if unchanged {
            return Ok(DevicesSetRelayResult { removed_devices: 0 });
        }
        let (keys, access) = match &pasted {
            None => (RelayKeys::default(), OwnRelayAccess::Unknown),
            Some(key) => self.keys_for(key)?,
        };
        let removed = match self.apply_relay(&keys, access) {
            Ok(removed) => removed,
            Err(error) => {
                if keys.manager.is_some() {
                    self.retire_own_key(&keys);
                }
                return Err(error);
            }
        };
        // The key this computer made for itself on the previous relay is no use now.
        if current.manager.is_some() {
            self.retire_own_key(&current);
        }
        logging::info(
            "remote_devices_relay_changed",
            serde_json::json!({
                "own_relay": keys.relay.is_some(),
                "can_manage": keys.manager.is_some(),
                "access": access_name(access),
                "removed_devices": removed,
            }),
        );
        Ok(DevicesSetRelayResult {
            removed_devices: u32::try_from(removed).unwrap_or(u32::MAX),
        })
    }

    pub(super) fn relay_people(&self) -> Result<DevicesRelayPeopleResult, ProtocolError> {
        let (manager, own_name) = self.manager()?;
        let people = self
            .inner
            .relay_api
            .people(&manager)
            .map_err(|error| relay_error(error, Asked::Manage))?;
        Ok(DevicesRelayPeopleResult {
            people: people
                .into_iter()
                .map(|person| relay_person(person, own_name.as_deref()))
                .collect(),
        })
    }

    pub(super) fn relay_add_person(
        &self,
        name: &str,
        can_manage: bool,
    ) -> Result<DevicesRelayAddPersonResult, ProtocolError> {
        let (manager, own_name) = self.manager()?;
        let added = self
            .inner
            .relay_api
            .add(&manager, name.trim(), can_manage)
            .map_err(|error| relay_error(error, Asked::Manage))?;
        logging::info(
            "remote_devices_relay_person_added",
            serde_json::json!({ "can_manage": added.person.can_manage }),
        );
        Ok(DevicesRelayAddPersonResult {
            key: added.key.line(),
            person: relay_person(added.person, own_name.as_deref()),
        })
    }

    pub(super) fn relay_revoke_person(&self, name: &str) -> Result<(), ProtocolError> {
        let (manager, own_name) = self.manager()?;
        if own_name.is_some_and(|own| same_name(&own, name)) {
            return Err(ProtocolError {
                code: ProtocolErrorCode::ValidationFailed,
                message: "This is this computer's own key. To stop using the relay, change the relay instead."
                    .to_string(),
                recoverable: true,
                target: field("name"),
            });
        }
        self.inner
            .relay_api
            .revoke(&manager, name.trim())
            .map_err(|error| relay_error(error, Asked::Manage))?;
        logging::info("remote_devices_relay_person_revoked", serde_json::json!({}));
        Ok(())
    }

    /// Asks the relay whether it still accepts this computer's key, and publishes
    /// the answer when it changed. An unreachable relay leaves the answer as it was.
    pub(crate) fn check_relay_access(&self) {
        let Some(relay) = self.state().stored.own_relay() else {
            return;
        };
        let access = match self.inner.relay_api.me(&relay) {
            Ok(_) => OwnRelayAccess::Accepted,
            Err(RelayApiError::Refused) => OwnRelayAccess::Refused,
            Err(_) => return,
        };
        let changed = {
            let mut state = self.state();
            // The relay may have changed while the relay answered.
            if state.stored.own_relay().as_ref() != Some(&relay) || state.relay_access == access {
                return;
            }
            state.relay_access = access;
            state.changed.clone()
        };
        logging::info(
            "remote_devices_relay_access_changed",
            serde_json::json!({ "access": access_name(access) }),
        );
        notify(changed);
    }

    /// The keys to keep for a pasted key, after the relay accepted it.
    fn keys_for(&self, key: &OwnRelay) -> Result<(RelayKeys, OwnRelayAccess), ProtocolError> {
        match self.inner.relay_api.me(key) {
            Ok(me) if me.can_manage => {
                let own = self.add_own_key(key)?;
                Ok((
                    RelayKeys {
                        relay: Some(own.key),
                        manager: Some(key.clone()),
                        name: Some(own.person.name),
                    },
                    OwnRelayAccess::Accepted,
                ))
            }
            Ok(me) => Ok((
                RelayKeys {
                    relay: Some(key.clone()),
                    manager: None,
                    name: Some(me.name),
                },
                OwnRelayAccess::Accepted,
            )),
            // A relay without the API, such as a stock iroh-relay, cannot say whether
            // it accepts the key until something dials through it.
            Err(RelayApiError::NoManagement) => Ok((
                RelayKeys {
                    relay: Some(key.clone()),
                    manager: None,
                    name: None,
                },
                OwnRelayAccess::Unknown,
            )),
            Err(error) => Err(relay_error(error, Asked::Key)),
        }
    }

    /// Adds the ordinary key this computer relays with, named after the computer.
    fn add_own_key(
        &self,
        manager: &OwnRelay,
    ) -> Result<super::relay_api::NewPerson, ProtocolError> {
        let base: String = self
            .inner
            .server_name
            .chars()
            .take(RELAY_NAME_CHARS - 2)
            .collect();
        let base = if base.trim().is_empty() {
            "Computer".to_string()
        } else {
            base
        };
        for attempt in 1..=OWN_NAME_ATTEMPTS {
            let name = if attempt == 1 {
                base.clone()
            } else {
                format!("{base} {attempt}")
            };
            match self.inner.relay_api.add(manager, &name, false) {
                Err(RelayApiError::NameTaken) => continue,
                result => return result.map_err(|error| relay_error(error, Asked::Key)),
            }
        }
        Err(relay_error(RelayApiError::NameTaken, Asked::Key))
    }

    /// Revokes the key this computer made for itself. Best effort: a key left behind
    /// only lets the relay be used, and a manager can revoke it from the list.
    fn retire_own_key(&self, keys: &RelayKeys) {
        let (Some(manager), Some(name)) = (&keys.manager, &keys.name) else {
            return;
        };
        if let Err(error) = self.inner.relay_api.revoke(manager, name) {
            logging::warn(
                "remote_devices_relay_own_key_left",
                serde_json::json!({ "error": error.class() }),
            );
        }
    }

    /// Keeps `keys`. A device knows only the relay it paired through, so every device
    /// pairs again; the identity goes with them, as when the last device is removed.
    fn apply_relay(
        &self,
        keys: &RelayKeys,
        access: OwnRelayAccess,
    ) -> Result<usize, ProtocolError> {
        let mut state = self.state();
        let mut stored = state.stored.clone();
        let removed: Vec<String> = stored
            .devices
            .drain(..)
            .map(|device| device.device_id)
            .collect();
        stored.forget_secret_key();
        stored.set_relay_keys(keys);
        self.inner.store.write(&stored).map_err(storage_error)?;
        state.stored = stored;
        state.relay_access = access;
        state.invite = None;
        state.connections.clear();
        state.connection_devices.clear();
        for device in &removed {
            if let Ok(device) = device.parse::<PublicKey>() {
                state.send(EdgeCommand::Disconnect(device));
            }
        }
        state.send(EdgeCommand::Reconcile);
        Ok(removed.len())
    }

    /// The manage key and this computer's own name on the relay.
    fn manager(&self) -> Result<(OwnRelay, Option<String>), ProtocolError> {
        let keys = self.state().stored.relay_keys();
        let manager = keys.manager.ok_or_else(|| ProtocolError {
            code: ProtocolErrorCode::CapabilityUnavailable,
            message: "This computer's relay key can't add or remove people".to_string(),
            recoverable: false,
            target: None,
        })?;
        Ok((manager, keys.name))
    }

    fn relay_ops(&self) -> MutexGuard<'_, ()> {
        self.inner
            .relay_ops
            .lock()
            .expect("relay operations lock poisoned")
    }
}

pub(super) fn access_name(access: OwnRelayAccess) -> &'static str {
    match access {
        OwnRelayAccess::Accepted => "accepted",
        OwnRelayAccess::Refused => "refused",
        OwnRelayAccess::Unknown => "unknown",
    }
}

fn parse_key(line: &str) -> Result<OwnRelay, ProtocolError> {
    OwnRelay::parse(line).map_err(|problem| ProtocolError {
        code: ProtocolErrorCode::ValidationFailed,
        message: format!("This is not a relay key: {problem}"),
        recoverable: true,
        target: field("relay"),
    })
}

/// The relay compares names without regard to case.
fn same_name(left: &str, right: &str) -> bool {
    left.trim().to_lowercase() == right.trim().to_lowercase()
}

fn relay_person(person: Person, own_name: Option<&str>) -> RelayPerson {
    RelayPerson {
        this_computer: own_name.is_some_and(|own| same_name(own, &person.name)),
        name: person.name,
        can_manage: person.can_manage,
    }
}

fn field(name: &str) -> Option<ErrorTarget> {
    Some(ErrorTarget {
        method: None,
        field: Some(name.to_string()),
        current_task: None,
    })
}

fn relay_error(error: RelayApiError, asked: Asked) -> ProtocolError {
    let (code, message, target) = match (error, asked) {
        (RelayApiError::Refused, Asked::Key) => (
            ProtocolErrorCode::ValidationFailed,
            "The relay doesn't accept this key",
            field("relay"),
        ),
        (RelayApiError::Refused, Asked::Manage) => (
            ProtocolErrorCode::Unauthorized,
            "The relay no longer accepts the key this computer manages it with",
            None,
        ),
        (RelayApiError::CannotManage, _) => (
            ProtocolErrorCode::CapabilityUnavailable,
            "This relay key can't add or remove people",
            None,
        ),
        (RelayApiError::InvalidName, _) => (
            ProtocolErrorCode::ValidationFailed,
            "Use a name of 1 to 40 characters",
            field("name"),
        ),
        (RelayApiError::NameTaken, Asked::Key) => (
            ProtocolErrorCode::Conflict,
            "The relay already has keys with this computer's name",
            None,
        ),
        (RelayApiError::NameTaken, Asked::Manage) => (
            ProtocolErrorCode::Conflict,
            "Someone with this name already has a key",
            field("name"),
        ),
        (RelayApiError::NoSuchPerson, _) => (
            ProtocolErrorCode::NotFound,
            "The relay has no key with this name",
            None,
        ),
        (RelayApiError::LastManager, _) => (
            ProtocolErrorCode::Conflict,
            "At least one key must be able to add and remove people",
            None,
        ),
        (RelayApiError::NoManagement, _) => (
            ProtocolErrorCode::CapabilityUnavailable,
            "This relay can't add or remove people",
            None,
        ),
        (RelayApiError::Unreachable, _) => (
            ProtocolErrorCode::CapabilityUnavailable,
            "OpenAIDE couldn't reach the relay. Check that it is running and try again.",
            None,
        ),
        (RelayApiError::Malformed, _) => (
            ProtocolErrorCode::Internal,
            "The relay gave an answer OpenAIDE can't read",
            None,
        ),
    };
    ProtocolError {
        code,
        message: message.to_string(),
        recoverable: !matches!(
            error,
            RelayApiError::CannotManage | RelayApiError::NoManagement
        ),
        target,
    }
}

#[cfg(test)]
#[path = "relay_management_tests.rs"]
mod tests;
