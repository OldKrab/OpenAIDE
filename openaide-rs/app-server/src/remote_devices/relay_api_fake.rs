//! An in-memory OpenAIDE relay for tests: the management API's rules without a
//! network.

use std::sync::{Arc, Mutex};

use openaide_remote_core::OwnRelay;

use super::{NewPerson, Person, RelayApi, RelayApiError};

pub(crate) const RELAY: &str = "https://relay.example.com/";

#[derive(Debug, Clone)]
struct Key {
    name: String,
    token: String,
    can_manage: bool,
}

#[derive(Debug, Default)]
struct Relay {
    keys: Vec<Key>,
    issued: u32,
    /// Answers like a stock iroh-relay, which has no management API.
    stock: bool,
    unreachable: bool,
}

/// Clones share one relay.
#[derive(Debug, Clone, Default)]
pub(crate) struct FakeRelay {
    relay: Arc<Mutex<Relay>>,
}

impl FakeRelay {
    /// A relay without the management API, such as a stock iroh-relay.
    pub(crate) fn stock() -> Self {
        let fake = Self::default();
        fake.relay().stock = true;
        fake
    }

    /// Issues a key directly, as `openaide-relay add` does, and returns its line.
    pub(crate) fn issue(&self, name: &str, can_manage: bool) -> String {
        let mut relay = self.relay();
        relay.issued += 1;
        let token = format!("token{}", relay.issued);
        relay.keys.push(Key {
            name: name.to_string(),
            token: token.clone(),
            can_manage,
        });
        format!("{RELAY}?token={token}")
    }

    pub(crate) fn names(&self) -> Vec<String> {
        self.relay()
            .keys
            .iter()
            .map(|key| key.name.clone())
            .collect()
    }

    pub(crate) fn set_unreachable(&self, unreachable: bool) {
        self.relay().unreachable = unreachable;
    }

    /// Revokes `name` as an operator would on the relay's own command line.
    pub(crate) fn revoke_directly(&self, name: &str) {
        self.relay().keys.retain(|key| key.name != name);
    }

    fn relay(&self) -> std::sync::MutexGuard<'_, Relay> {
        self.relay.lock().unwrap()
    }

    fn holder(&self, key: &OwnRelay, manage: bool) -> Result<Key, RelayApiError> {
        let relay = self.relay();
        if relay.unreachable {
            return Err(RelayApiError::Unreachable);
        }
        if relay.stock {
            return Err(RelayApiError::NoManagement);
        }
        let holder = relay
            .keys
            .iter()
            .find(|held| held.token == key.password())
            .cloned()
            .ok_or(RelayApiError::Refused)?;
        if manage && !holder.can_manage {
            return Err(RelayApiError::CannotManage);
        }
        Ok(holder)
    }
}

impl RelayApi for FakeRelay {
    fn me(&self, key: &OwnRelay) -> Result<Person, RelayApiError> {
        let holder = self.holder(key, false)?;
        Ok(Person {
            name: holder.name,
            can_manage: holder.can_manage,
        })
    }

    fn people(&self, key: &OwnRelay) -> Result<Vec<Person>, RelayApiError> {
        self.holder(key, true)?;
        Ok(self
            .relay()
            .keys
            .iter()
            .map(|key| Person {
                name: key.name.clone(),
                can_manage: key.can_manage,
            })
            .collect())
    }

    fn add(
        &self,
        key: &OwnRelay,
        name: &str,
        can_manage: bool,
    ) -> Result<NewPerson, RelayApiError> {
        self.holder(key, true)?;
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 40 {
            return Err(RelayApiError::InvalidName);
        }
        if self
            .relay()
            .keys
            .iter()
            .any(|key| key.name.to_lowercase() == name.to_lowercase())
        {
            return Err(RelayApiError::NameTaken);
        }
        let line = self.issue(name, can_manage);
        Ok(NewPerson {
            person: Person {
                name: name.to_string(),
                can_manage,
            },
            key: OwnRelay::parse(&line).unwrap(),
        })
    }

    fn revoke(&self, key: &OwnRelay, name: &str) -> Result<(), RelayApiError> {
        self.holder(key, true)?;
        let mut relay = self.relay();
        let index = relay
            .keys
            .iter()
            .position(|key| key.name.to_lowercase() == name.to_lowercase())
            .ok_or(RelayApiError::NoSuchPerson)?;
        let managers = relay.keys.iter().filter(|key| key.can_manage).count();
        if relay.keys[index].can_manage && managers == 1 {
            return Err(RelayApiError::LastManager);
        }
        relay.keys.remove(index);
        Ok(())
    }
}
