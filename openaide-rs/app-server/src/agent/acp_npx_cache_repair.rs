//! Repairs an `npx` cache entry that an interrupted install left without its command.
//!
//! npm owns the `npx` cache for `npx -y` launches: OpenCode's fallback, Custom Agents, and
//! runtimes built without the managed provisioner (ADR 0007). When an install is
//! killed after npm extracted the package but before it linked the package commands,
//! npm still treats the entry as installed, so every later launch fails to find the
//! command and exits before ACP `initialize`. Nothing in npm repairs that state. After
//! such an early exit, App Server removes only that exact entry, and only when no npm
//! install currently holds it, so the next launch installs the package again.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use sha2::{Digest, Sha512};

use crate::agent::acp_agent_config::AcpAgentConfig;
use crate::logging;

#[cfg(test)]
#[path = "acp_npx_cache_repair_tests.rs"]
mod tests;

/// npm refreshes `concurrency.lock` every second while it installs and treats a lock
/// older than this as abandoned, so a fresher lock means an install is in progress.
const NPM_LOCK_STALE_AFTER: Duration = Duration::from_secs(60);
/// npm releases without the install lock give no ownership signal; a recently changed
/// `node_modules` is treated as an install that may still be writing.
const RECENT_INSTALL_WRITE: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NpxPackageSpec {
    /// The exact argument passed to `npx`, which npm hashes into the entry name.
    spec: String,
    name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NpxCacheRepairOutcome {
    /// The entry lacked a package command and was removed.
    Repaired,
    /// Every package command is present; the early exit has another cause.
    Healthy,
    /// npm will install the package itself on the next launch.
    PackageNotInstalled,
    EntryNotFound,
    InstallInProgress,
    CacheNotFound,
    Failed,
}

impl NpxCacheRepairOutcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Repaired => "repaired",
            Self::Healthy => "healthy",
            Self::PackageNotInstalled => "package_not_installed",
            Self::EntryNotFound => "entry_not_found",
            Self::InstallInProgress => "install_in_progress",
            Self::CacheNotFound => "cache_not_found",
            Self::Failed => "failed",
        }
    }
}

/// Runs after an `npx` launch exited before ACP `initialize`. Launches that are not a
/// single exact-version `npx -y` package are left alone.
pub(super) fn repair_after_early_exit(
    agent_id: &str,
    config: &AcpAgentConfig,
    process_operation_id: &str,
) {
    let Some(spec) = npx_package_spec(&config.command, &config.args) else {
        return;
    };
    let started_at = Instant::now();
    let outcome = match npm_cache_root() {
        Some(cache_root) => repair_entry(&cache_root, &spec, SystemTime::now()),
        None => NpxCacheRepairOutcome::CacheNotFound,
    };
    let fields = serde_json::json!({
        "operation": "agent/npx_cache_repair",
        "operation_id": process_operation_id,
        "agent_id": agent_id,
        "outcome": outcome.as_str(),
        "duration_ms": started_at.elapsed().as_millis(),
    });
    match outcome {
        NpxCacheRepairOutcome::Repaired | NpxCacheRepairOutcome::Failed => {
            logging::warn("acp_npx_cache_repair_completed", fields)
        }
        _ => logging::info("acp_npx_cache_repair_completed", fields),
    }
}

/// Recognizes `npx -y <name>@<version>`, the launch shape of an exactly pinned package.
pub(super) fn npx_package_spec(command: &str, args: &[String]) -> Option<NpxPackageSpec> {
    let launcher = Path::new(command).file_stem()?.to_str()?;
    if !launcher.eq_ignore_ascii_case("npx") {
        return None;
    }
    let [yes, spec, ..] = args else {
        return None;
    };
    if yes != "-y" {
        return None;
    }
    // A scoped name starts with `@`, so the version separator is the last `@` after it.
    let (name, version) = spec.get(1..)?.rsplit_once('@')?;
    let name = format!("{}{name}", &spec[..1]);
    let exact_version = !version.is_empty()
        && version.starts_with(|character: char| character.is_ascii_digit())
        && version.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '+')
        });
    exact_version.then(|| NpxPackageSpec {
        spec: spec.clone(),
        name,
    })
}

/// Mirrors npm's default cache location and its environment override.
// TODO: Read the effective cache from npm (`npm config get cache`) once a launch-time npm
// probe exists; the default and environment cover stock installs but miss `.npmrc` overrides.
fn npm_cache_root() -> Option<PathBuf> {
    for name in ["npm_config_cache", "NPM_CONFIG_CACHE"] {
        if let Some(value) = std::env::var_os(name).filter(|value| !value.is_empty()) {
            return Some(PathBuf::from(value));
        }
    }
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(|root| PathBuf::from(root).join("npm-cache"))
    } else {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".npm"))
    }
}

/// npm names the entry by the first 16 hex digits of the SHA-512 of the sorted specs.
pub(super) fn npx_entry_name(spec: &NpxPackageSpec) -> String {
    Sha512::digest(spec.spec.as_bytes())
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(super) fn repair_entry(
    cache_root: &Path,
    spec: &NpxPackageSpec,
    now: SystemTime,
) -> NpxCacheRepairOutcome {
    if !cache_root.is_dir() {
        return NpxCacheRepairOutcome::CacheNotFound;
    }
    let entry = cache_root.join("_npx").join(npx_entry_name(spec));
    if !entry.is_dir() {
        return NpxCacheRepairOutcome::EntryNotFound;
    }
    if install_may_be_running(&entry, now) {
        return NpxCacheRepairOutcome::InstallInProgress;
    }
    let node_modules = entry.join("node_modules");
    let Some(commands) = package_commands(&node_modules.join(&spec.name), &spec.name) else {
        return NpxCacheRepairOutcome::PackageNotInstalled;
    };
    let bin = node_modules.join(".bin");
    let missing_command = commands.iter().any(|command| {
        // `metadata` follows the link, so a dangling link counts as missing.
        fs::metadata(bin.join(command)).is_err()
            || (cfg!(windows) && fs::metadata(bin.join(format!("{command}.cmd"))).is_err())
    });
    if !missing_command {
        return NpxCacheRepairOutcome::Healthy;
    }
    match fs::remove_dir_all(&entry) {
        Ok(()) => NpxCacheRepairOutcome::Repaired,
        Err(_) => NpxCacheRepairOutcome::Failed,
    }
}

fn install_may_be_running(entry: &Path, now: SystemTime) -> bool {
    let changed_within = |path: PathBuf, window: Duration| {
        fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .is_ok_and(|modified| now.duration_since(modified).unwrap_or_default() < window)
    };
    changed_within(entry.join("concurrency.lock"), NPM_LOCK_STALE_AFTER)
        || changed_within(entry.join("node_modules"), RECENT_INSTALL_WRITE)
}

/// Commands npm links for the package, or `None` when the package is not extracted.
fn package_commands(package_root: &Path, name: &str) -> Option<Vec<String>> {
    let manifest = fs::read_to_string(package_root.join("package.json")).ok()?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest).ok()?;
    Some(match manifest.get("bin") {
        // A string `bin` is linked under the unscoped package name.
        Some(serde_json::Value::String(_)) => {
            vec![name.rsplit('/').next().unwrap_or(name).to_string()]
        }
        Some(serde_json::Value::Object(commands)) => commands.keys().cloned().collect(),
        _ => Vec::new(),
    })
}
