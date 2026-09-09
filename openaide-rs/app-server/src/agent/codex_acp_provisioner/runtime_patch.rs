//! The embedded manifest identifies one immutable managed adapter patch. Cached
//! launches verify installed bytes in Rust; Node runs only while staging npm output.

use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};

use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::{package_root, provisioning_error, provisioning_io_error};
use crate::agent::acp_agent_config::resolved_command_or_name;
use crate::protocol::errors::RuntimeError;

const PATCH_INSTALLER: &str =
    include_str!("../../../assets/codex-acp-runtime/apply-session-recovery.mjs");
const PATCH_HELPER: &str = include_str!("../../../assets/codex-acp-runtime/session-recovery.mjs");
const PATCH_MANIFEST: &str =
    include_str!("../../../assets/codex-acp-runtime/session-recovery-manifest.json");

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RuntimeManifest {
    pub(super) package_name: String,
    pub(super) package_version: String,
    pub(super) runtime_id: String,
    patched_sha256: String,
    helper_sha256: String,
}

pub(super) fn manifest() -> &'static RuntimeManifest {
    static MANIFEST: OnceLock<RuntimeManifest> = OnceLock::new();
    MANIFEST
        .get_or_init(|| serde_json::from_str(PATCH_MANIFEST).expect("embedded runtime manifest"))
}

#[derive(Clone)]
pub(super) struct ArtifactDigests {
    index: String,
    helper: String,
}

impl Default for ArtifactDigests {
    fn default() -> Self {
        Self {
            index: manifest().patched_sha256.clone(),
            helper: manifest().helper_sha256.clone(),
        }
    }
}

impl ArtifactDigests {
    #[cfg(test)]
    pub(super) fn for_fixture(index: &[u8], helper: &[u8]) -> Self {
        Self {
            index: format!("{:x}", Sha256::digest(index)),
            helper: format!("{:x}", Sha256::digest(helper)),
        }
    }

    pub(super) fn validate(&self, version_root: &Path) -> Result<(), RuntimeError> {
        let dist = package_root(version_root).join("dist");
        for (name, expected) in [
            ("index.js", &self.index),
            ("openaide-session-recovery.mjs", &self.helper),
        ] {
            let mut file = File::open(dist.join(name)).map_err(provisioning_io_error)?;
            let mut digest = Sha256::new();
            let mut buffer = [0; 16 * 1024];
            loop {
                let length = file.read(&mut buffer).map_err(provisioning_io_error)?;
                if length == 0 {
                    break;
                }
                digest.update(&buffer[..length]);
            }
            if &format!("{:x}", digest.finalize()) != expected {
                return Err(provisioning_error(
                    "installed Codex integration did not match the managed patch".to_string(),
                ));
            }
        }
        Ok(())
    }
}

pub(super) fn write_assets(destination: &Path) -> Result<(), String> {
    for (name, content) in [
        ("apply-session-recovery.mjs", PATCH_INSTALLER),
        ("session-recovery.mjs", PATCH_HELPER),
        ("session-recovery-manifest.json", PATCH_MANIFEST),
    ] {
        fs::write(destination.join(name), content)
            .map_err(|_| "managed patch asset could not be written".to_string())?;
    }
    Ok(())
}

pub(super) fn apply(destination: &Path, deadline: Instant) -> Result<(), String> {
    let mut child = Command::new(resolved_command_or_name("node"))
        // Resolve both paths from staging: relative storage roots and Windows
        // verbatim filesystem paths must not become ambiguous Node arguments.
        .args(["apply-session-recovery.mjs", "."])
        .current_dir(destination)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "managed patch installer could not be started".to_string())?;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(_)) => return Err("managed patch installation failed".to_string()),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(25)),
            status => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(if status.is_err() {
                    "managed patch installation status was unavailable"
                } else {
                    "managed patch installation timed out"
                }
                .to_string());
            }
        }
    }
}
