//! The folder that belongs to the App Shell using other Computers (ADR-0082): its
//! device key and the Computers it is paired with. Only the owner can read it, and
//! it stays apart from the App Server's own state.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use openaide_remote_client::{
    device_key_from_text, device_key_text, new_device_key, OwnRelay, SecretKey,
};
use serde::{Deserialize, Serialize};

const KEY_FILE: &str = "device.key";
const COMPUTERS_FILE: &str = "computers.json";

/// A paired Computer as the user knows it: its App Server's public key and name.
/// Browsers are shown this, so it holds nothing secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Computer {
    pub id: String,
    pub name: String,
}

/// A Computer as the folder keeps it: with the Own Relay line, password included,
/// that its pairing code carried (ADR-0085). The line never leaves the folder.
#[derive(Clone, Serialize, Deserialize)]
struct StoredComputer {
    #[serde(flatten)]
    computer: Computer,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    relay: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
struct Computers {
    computers: Vec<StoredComputer>,
}

pub struct Folder {
    path: PathBuf,
    /// Pairing and forgetting rewrite the list; one at a time keeps both changes.
    writing: Mutex<()>,
}

impl Folder {
    /// Opens the folder, creating it readable by the owner only.
    pub fn open(path: &Path) -> io::Result<Self> {
        fs::create_dir_all(path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
        }
        Ok(Self {
            path: path.to_path_buf(),
            writing: Mutex::new(()),
        })
    }

    /// The device key, made on first use. Returns whether it was just made.
    pub fn key(&self) -> io::Result<(SecretKey, bool)> {
        let path = self.path.join(KEY_FILE);
        match fs::read_to_string(&path) {
            Ok(text) => device_key_from_text(&text)
                .map(|key| (key, false))
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "unreadable device key")),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let key = new_device_key();
                write_private(&path, device_key_text(&key).as_bytes())?;
                Ok((key, true))
            }
            Err(error) => Err(error),
        }
    }

    pub fn computers(&self) -> io::Result<Vec<Computer>> {
        Ok(self
            .stored()?
            .into_iter()
            .map(|stored| stored.computer)
            .collect())
    }

    /// The Own Relay the Computer `id` is reached through, or `None` for the public
    /// relays. A line that no longer reads is treated as damage.
    pub fn relay(&self, id: &str) -> io::Result<Option<OwnRelay>> {
        let Some(line) = self
            .stored()?
            .into_iter()
            .find(|stored| stored.computer.id == id)
            .and_then(|stored| stored.relay)
        else {
            return Ok(None);
        };
        OwnRelay::parse(&line)
            .map(Some)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "unreadable relay"))
    }

    fn stored(&self) -> io::Result<Vec<StoredComputer>> {
        match fs::read(self.path.join(COMPUTERS_FILE)) {
            Ok(bytes) => serde_json::from_slice::<Computers>(&bytes)
                .map(|stored| stored.computers)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(error),
        }
    }

    pub fn find(&self, id: &str) -> io::Result<Option<Computer>> {
        Ok(self
            .computers()?
            .into_iter()
            .find(|computer| computer.id == id))
    }

    /// Adds a Computer, or renames it and replaces its relay when pairing with it again.
    pub fn remember(&self, computer: Computer, relay: Option<&OwnRelay>) -> io::Result<()> {
        let _writing = self
            .writing
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let mut computers = self.stored()?;
        let computer = StoredComputer {
            computer,
            relay: relay.map(OwnRelay::line),
        };
        match computers
            .iter_mut()
            .find(|known| known.computer.id == computer.computer.id)
        {
            Some(known) => *known = computer,
            None => computers.push(computer),
        }
        self.write(computers)
    }

    /// Removes a Computer; returns whether it was paired.
    pub fn forget(&self, id: &str) -> io::Result<bool> {
        let _writing = self
            .writing
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let mut computers = self.stored()?;
        let before = computers.len();
        computers.retain(|stored| stored.computer.id != id);
        if computers.len() == before {
            return Ok(false);
        }
        self.write(computers)?;
        Ok(true)
    }

    fn write(&self, computers: Vec<StoredComputer>) -> io::Result<()> {
        let bytes = serde_json::to_vec_pretty(&Computers { computers })
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        write_private(&self.path.join(COMPUTERS_FILE), &bytes)
    }
}

/// Replaces `path` whole, so a crash leaves the old or the new content, never a mix.
fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let partial = path.with_extension("partial");
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&partial)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(&partial, path)
}
