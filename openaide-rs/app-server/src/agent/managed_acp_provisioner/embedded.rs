//! Adapter packages compiled into the App Server from fork submodules (ADR-0079).
//!
//! `build.rs` embeds the files `npm run build:adapters` staged and their build-time
//! SHA-256 digests. The provisioner unpacks one into a staging installation beside
//! its npm-installed dependencies, and every launch re-checks the installed bytes
//! against the same digests, so a cached installation can never run other code.

use std::fs::{self, File};
use std::io::{ErrorKind, Read};
use std::path::Path;

use sha2::{Digest, Sha256};

pub(crate) struct EmbeddedAdapter {
    /// The `adapters/adapters.json` id.
    pub(crate) id: &'static str,
    pub(crate) name: &'static str,
    pub(crate) version: &'static str,
    /// The full fork commit the package was built from.
    pub(crate) commit: &'static str,
    pub(crate) files: &'static [EmbeddedFile],
}

pub(crate) struct EmbeddedFile {
    /// Relative to the package root, with `/` separators.
    pub(crate) path: &'static str,
    pub(crate) sha256: &'static str,
    pub(crate) bytes: &'static [u8],
}

include!(concat!(env!("OUT_DIR"), "/embedded_adapters.rs"));

/// Why an installed adapter does not match its embedded package; a stable
/// diagnostic class with no path or content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AdapterFault {
    MissingFile,
    DigestMismatch,
    Io,
}

impl AdapterFault {
    pub(super) fn kind(self) -> &'static str {
        match self {
            Self::MissingFile => "missing_file",
            Self::DigestMismatch => "digest_mismatch",
            Self::Io => "io",
        }
    }
}

pub(super) fn adapter(id: &str) -> &'static EmbeddedAdapter {
    EMBEDDED_ADAPTERS
        .iter()
        .find(|adapter| adapter.id == id)
        .expect("build.rs embeds every adapter in adapters/adapters.json")
}

impl EmbeddedAdapter {
    pub(super) fn short_commit(&self) -> &'static str {
        &self.commit[..12]
    }

    /// The immutable installation directory: `<version>-<12-character commit>`.
    pub(super) fn runtime_id(&self) -> String {
        format!("{}-{}", self.version, self.short_commit())
    }

    /// Writes the package into `package_root`, replacing anything there. The
    /// written bytes are checked by [`Self::validate`] before publication.
    pub(super) fn unpack(&self, package_root: &Path) -> Result<(), AdapterFault> {
        match fs::remove_dir_all(package_root) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(_) => return Err(AdapterFault::Io),
        }
        for file in self.files {
            let target = package_root.join(file.path);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|_| AdapterFault::Io)?;
            }
            fs::write(&target, file.bytes).map_err(|_| AdapterFault::Io)?;
        }
        Ok(())
    }

    /// Checks every installed file against its build-time digest.
    pub(super) fn validate(&self, package_root: &Path) -> Result<(), AdapterFault> {
        for file in self.files {
            let digest = file_digest(&package_root.join(file.path))?;
            if digest != file.sha256 {
                return Err(AdapterFault::DigestMismatch);
            }
        }
        Ok(())
    }
}

fn file_digest(path: &Path) -> Result<String, AdapterFault> {
    let mut file = File::open(path).map_err(|error| match error.kind() {
        ErrorKind::NotFound => AdapterFault::MissingFile,
        _ => AdapterFault::Io,
    })?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let length = file.read(&mut buffer).map_err(|_| AdapterFault::Io)?;
        if length == 0 {
            break;
        }
        digest.update(&buffer[..length]);
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}
