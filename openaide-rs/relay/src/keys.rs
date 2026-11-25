//! The people a relay admits, each with their own relay key, kept in
//! `<state dir>/keys.json`.
//!
//! The file is the only source of truth. The `serve` process and an operator's commands
//! both change it, so every change takes the state directory's lock, rereads the file,
//! and replaces it atomically. Readers read the file on every access, which is how `serve`
//! sees a command's change without a restart, and parse it again only when its bytes
//! changed. The file is small, and comparing bytes cannot miss a change the way a
//! modification time of coarse resolution can.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::files::{self, DirLock};

pub const KEYS_FILE: &str = "keys.json";
pub const MAX_NAME_CHARS: usize = 40;
const TOKEN_BYTES: usize = 24;
const FORMAT_VERSION: u32 = 1;

/// A person the relay admits.
#[derive(Clone, PartialEq, Eq)]
pub struct Person {
    /// As typed, trimmed. Unique among people when compared case-insensitively.
    pub name: String,
    pub token: String,
    /// Whether this key may list, add and revoke keys.
    pub can_manage: bool,
    /// Seconds since the Unix epoch.
    pub created_at: u64,
}

impl Person {
    /// See [`crate::log::key_id`].
    pub fn key_id(&self) -> String {
        crate::log::key_id(&self.token)
    }

    /// The full SHA-256 of the token, which tells keys apart without holding one.
    pub fn token_digest(&self) -> TokenDigest {
        TokenDigest::of(&self.token)
    }
}

impl fmt::Debug for Person {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Person")
            .field("key_id", &self.key_id())
            .field("can_manage", &self.can_manage)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TokenDigest([u8; 32]);

impl TokenDigest {
    pub fn of(token: &str) -> Self {
        Self(Sha256::digest(token.as_bytes()).into())
    }

    pub fn key_id(&self) -> String {
        crate::log::short_hex(&self.0)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct KeysFile {
    version: u32,
    people: Vec<StoredPerson>,
}

#[derive(Serialize, Deserialize)]
struct StoredPerson {
    name: String,
    token: String,
    can_manage: bool,
    created_at: u64,
}

impl fmt::Debug for StoredPerson {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("StoredPerson")
    }
}

/// Why the key file could not be read or written. Holds no file content.
#[derive(Debug)]
pub enum StoreError {
    Io(io::Error),
    /// The file is not a key list this program wrote.
    Corrupt,
}

impl StoreError {
    pub fn class(&self) -> &'static str {
        match self {
            Self::Io(error) => crate::log::io_class(error),
            Self::Corrupt => "corrupt",
        }
    }
}

impl fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "cannot use the key file: {error}"),
            Self::Corrupt => formatter.write_str("the key file is damaged"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<io::Error> for StoreError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug)]
pub enum AddError {
    InvalidName,
    NameTaken,
    Store(StoreError),
}

impl From<StoreError> for AddError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

#[derive(Debug)]
pub enum RevokeError {
    NoSuchPerson,
    /// Revoking would leave no key that can manage the relay.
    LastManager,
    Store(StoreError),
}

impl From<StoreError> for RevokeError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

#[derive(Debug)]
struct Loaded {
    /// The file's bytes; empty when there is no file.
    raw: Vec<u8>,
    people: Arc<Vec<Person>>,
}

#[derive(Debug)]
pub struct KeyStore {
    dir: PathBuf,
    path: PathBuf,
    cache: Mutex<Option<Loaded>>,
}

impl KeyStore {
    pub fn open(state_dir: &Path) -> Self {
        Self {
            dir: state_dir.to_path_buf(),
            path: state_dir.join(KEYS_FILE),
            cache: Mutex::new(None),
        }
    }

    /// Everyone, in the order they were added. A missing file is an empty list.
    pub fn people(&self) -> Result<Arc<Vec<Person>>, StoreError> {
        let raw = self.read_raw()?;
        let mut cache = self
            .cache
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if let Some(loaded) = cache.as_ref().filter(|loaded| loaded.raw == raw) {
            return Ok(loaded.people.clone());
        }
        let people = Arc::new(people_from(parse(&raw)?));
        *cache = Some(Loaded {
            raw,
            people: people.clone(),
        });
        Ok(people)
    }

    /// The person whose key is `token`. Every stored token is compared in full, so the
    /// time taken does not tell how much of a guess was right.
    pub fn person_for_token(&self, token: &str) -> Result<Option<Person>, StoreError> {
        let people = self.people()?;
        let mut found = None;
        for person in people.iter() {
            if constant_time_eq(person.token.as_bytes(), token.as_bytes()) {
                found = Some(person.clone());
            }
        }
        Ok(found)
    }

    /// The person whose name matches `name`, ignoring case and surrounding spaces.
    pub fn person_named(&self, name: &str) -> Result<Option<Person>, StoreError> {
        let wanted = fold(name.trim());
        Ok(self
            .people()?
            .iter()
            .find(|person| fold(&person.name) == wanted)
            .cloned())
    }

    /// Adds a person with a fresh key.
    pub fn add(&self, name: &str, can_manage: bool) -> Result<Person, AddError> {
        let name = validate_name(name).ok_or(AddError::InvalidName)?;
        self.change(|people| {
            if people
                .iter()
                .any(|person| fold(&person.name) == fold(&name))
            {
                return Err(AddError::NameTaken);
            }
            let stored = StoredPerson {
                name: name.clone(),
                token: new_token().map_err(|error| AddError::Store(StoreError::Io(error)))?,
                can_manage,
                created_at: now_unix(),
            };
            let person = person_from(&stored);
            people.push(stored);
            Ok(person)
        })
    }

    /// Removes a person, refusing to remove the last one who can manage.
    pub fn revoke(&self, name: &str) -> Result<Person, RevokeError> {
        let wanted = fold(name.trim());
        self.change(|people| {
            let index = people
                .iter()
                .position(|person| fold(&person.name) == wanted)
                .ok_or(RevokeError::NoSuchPerson)?;
            let managers = people.iter().filter(|person| person.can_manage).count();
            if people[index].can_manage && managers <= 1 {
                return Err(RevokeError::LastManager);
            }
            Ok(person_from(&people.remove(index)))
        })
    }

    /// Runs one read-modify-write under the directory lock. The file is written only when
    /// `apply` succeeds.
    fn change<T, E: From<StoreError>>(
        &self,
        apply: impl FnOnce(&mut Vec<StoredPerson>) -> Result<T, E>,
    ) -> Result<T, E> {
        let _lock = DirLock::acquire(&self.dir).map_err(StoreError::from)?;
        let mut file = self.read_file()?;
        let outcome = apply(&mut file.people)?;
        let mut text = serde_json::to_vec_pretty(&file).map_err(|_| StoreError::Corrupt)?;
        text.push(b'\n');
        files::write_private(&self.path, &text).map_err(StoreError::from)?;
        Ok(outcome)
    }

    fn read_file(&self) -> Result<KeysFile, StoreError> {
        parse(&self.read_raw()?)
    }

    fn read_raw(&self) -> Result<Vec<u8>, StoreError> {
        match std::fs::read(&self.path) {
            Ok(bytes) => Ok(bytes),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(error.into()),
        }
    }
}

/// Reads the key file's bytes; no bytes is an empty list.
fn parse(raw: &[u8]) -> Result<KeysFile, StoreError> {
    if raw.is_empty() {
        return Ok(KeysFile {
            version: FORMAT_VERSION,
            people: Vec::new(),
        });
    }
    let file: KeysFile = serde_json::from_slice(raw).map_err(|_| StoreError::Corrupt)?;
    if file.version != FORMAT_VERSION {
        return Err(StoreError::Corrupt);
    }
    Ok(file)
}

/// Trims a name and checks it: 1 to 40 characters, none of them control characters.
pub fn validate_name(raw: &str) -> Option<String> {
    let name = raw.trim();
    let length = name.chars().count();
    ((1..=MAX_NAME_CHARS).contains(&length) && !name.chars().any(char::is_control))
        .then(|| name.to_string())
}

fn fold(name: &str) -> String {
    name.to_lowercase()
}

fn people_from(file: KeysFile) -> Vec<Person> {
    file.people.iter().map(person_from).collect()
}

fn person_from(stored: &StoredPerson) -> Person {
    Person {
        name: stored.name.clone(),
        token: stored.token.clone(),
        can_manage: stored.can_manage,
        created_at: stored.created_at,
    }
}

/// 24 random bytes from the system's secure generator, as 48 lowercase hex characters.
fn new_token() -> io::Result<String> {
    let mut bytes = [0u8; TOKEN_BYTES];
    crate::tls::provider()
        .secure_random
        .fill(&mut bytes)
        .map_err(|_| io::Error::other("no secure random source"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0u8, |difference, (a, b)| difference | (a ^ b))
        == 0
}

#[cfg(test)]
#[path = "keys_tests.rs"]
mod tests;
