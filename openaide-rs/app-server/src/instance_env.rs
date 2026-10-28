//! The environment variables that configure this App Server instance: its
//! state and runtime roots, protocol mode, project roots and ACP diagnostics.
//!
//! `main` takes them out of the process environment before anything else
//! runs, so no process the App Server starts in a workspace (an Agent, a
//! Terminal shell, git, npm) inherits them. An App Server started there, for
//! example by an Agent trying a build, must not find this instance's state
//! root, join it, and replace it when its version differs. The App Server
//! passes them on only when it starts itself again ([`for_own_launch`]). See
//! docs/adr/0072-workspace-processes-do-not-inherit-instance-settings.md.
//!
//! Reads go through [`var_os`]. A process that never took them, such as a
//! unit test, reads its environment as before.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::sync::OnceLock;

pub const STORAGE_ROOT: &str = "OPENAIDE_STORAGE_ROOT";
pub const RUNTIME_ROOT: &str = "OPENAIDE_RUNTIME_ROOT";
pub const PROTOCOL_MODE: &str = "OPENAIDE_APP_SERVER_PROTOCOL";
pub const LEGACY_PROTOCOL_MODE: &str = "OPENAIDE_RUNTIME_PROTOCOL";
pub const PROJECT_ROOTS: &str = "OPENAIDE_PROJECT_ROOTS";
pub const ACP_TRACE: &str = "OPENAIDE_ACP_TRACE";
pub const ACP_TRACE_DIR: &str = "OPENAIDE_ACP_TRACE_DIR";
pub const ACP_NATIVE_SUBAGENTS: &str = "OPENAIDE_ACP_NATIVE_SUBAGENTS";

/// Every name above. Other `OPENAIDE_*` variables, such as a test fixture's,
/// belong to whoever set them and still reach child processes.
pub const NAMES: [&str; 8] = [
    STORAGE_ROOT,
    RUNTIME_ROOT,
    PROTOCOL_MODE,
    LEGACY_PROTOCOL_MODE,
    PROJECT_ROOTS,
    ACP_TRACE,
    ACP_TRACE_DIR,
    ACP_NATIVE_SUBAGENTS,
];

static TAKEN: OnceLock<BTreeMap<&'static str, OsString>> = OnceLock::new();

/// Moves the instance settings from the process environment into this module
/// and returns how many were set. Call it first in `main`, before any thread
/// starts: changing the environment while another thread reads it is unsound
/// on some platforms. Later calls change nothing.
pub fn take_from_process() -> usize {
    TAKEN
        .get_or_init(|| {
            NAMES
                .iter()
                .filter_map(|name| {
                    let value = std::env::var_os(name)?;
                    std::env::remove_var(name);
                    Some((*name, value))
                })
                .collect()
        })
        .len()
}

pub fn var_os(name: &str) -> Option<OsString> {
    match TAKEN.get() {
        Some(taken) => taken.get(name).cloned(),
        None => std::env::var_os(name),
    }
}

pub fn var(name: &str) -> Option<String> {
    var_os(name)?.into_string().ok()
}

/// The instance settings for a process that is this App Server started again.
pub fn for_own_launch() -> Vec<(&'static str, OsString)> {
    NAMES
        .iter()
        .filter_map(|name| var_os(name).map(|value| (*name, value)))
        .collect()
}
