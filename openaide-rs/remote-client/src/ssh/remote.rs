//! The commands an App Shell runs on an SSH Computer and the answers it reads.
//!
//! The SSH server hands each command line to the user's login shell, which may
//! be bash, zsh, fish or tcsh. Every command is therefore one `sh -c '…'` line:
//! the scripts hold no single quote, no `!` and no newline, which those shells
//! all pass to `sh` unchanged, and every argument is a single-quoted word.

use serde::Deserialize;

/// The versioned install directory, shared with the Desktop's WSL runtime.
const INSTALL_DIRECTORY: &str = r#"d="$HOME/.local/share/openaide/app-server/$1""#;

/// Prints `key=value` facts: the platform, the installed binary's checksum, and
/// whether logind ends the user's processes at logout.
const PROBE: &str = r#"b="$d/openaide-app-server"; echo "platform=$(uname -sm)"; if [ -x "$b" ] && command -v sha256sum >/dev/null 2>&1; then echo "installed=$(sha256sum "$b" | cut -d " " -f 1)"; fi; if command -v loginctl >/dev/null 2>&1; then echo "kill_user_processes=$(loginctl show-session -p KillUserProcesses --value 2>/dev/null)"; echo "linger=$(loginctl show-user "$(id -un)" -p Linger --value 2>/dev/null)"; fi"#;

/// Receives the binary on stdin and moves it into place only if its checksum
/// matches, so an interrupted or corrupted upload never becomes the install.
const INSTALL: &str = r#"command -v sha256sum >/dev/null 2>&1 || { echo checksum_tool_missing; exit 0; }; mkdir -p "$d" || { echo install_failed; exit 0; }; t="$d/openaide-app-server.upload.$$"; cat > "$t" || { rm -f "$t"; echo install_failed; exit 0; }; [ "$(sha256sum "$t" | cut -d " " -f 1)" = "$2" ] || { rm -f "$t"; echo checksum_mismatch; exit 0; }; chmod 700 "$t" && mv -f "$t" "$d/openaide-app-server" && echo installed || { rm -f "$t"; echo install_failed; }"#;

/// Runs the installed binary's `keep` command with the remaining arguments.
const KEEP: &str = r#"b="$d/openaide-app-server"; shift; exec "$b" keep "$@""#;

pub(crate) fn probe(version: &str) -> String {
    command(PROBE, &[version])
}

pub(crate) fn install(version: &str, sha256: &str) -> String {
    command(INSTALL, &[version, sha256])
}

pub(crate) fn keep(version: &str, arguments: &[&str]) -> String {
    let mut all = vec![version];
    all.extend_from_slice(arguments);
    command(KEEP, &all)
}

fn command(script: &str, arguments: &[&str]) -> String {
    let mut line = format!("sh -c '{INSTALL_DIRECTORY}; {script}' openaide");
    for argument in arguments {
        line.push(' ');
        line.push_str(&quote(argument));
    }
    line
}

fn quote(argument: &str) -> String {
    format!("'{}'", argument.replace('\'', r"'\''"))
}

/// What the probe found on the SSH Computer.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Probe {
    pub(crate) platform: String,
    pub(crate) installed_sha256: Option<String>,
    pub(crate) kill_user_processes: Option<bool>,
    pub(crate) linger: Option<bool>,
}

impl Probe {
    pub(crate) fn parse(stdout: &[u8]) -> Self {
        let mut probe = Self::default();
        for line in String::from_utf8_lossy(stdout).lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key {
                "platform" => probe.platform = value.to_string(),
                "installed" if !value.is_empty() => {
                    probe.installed_sha256 = Some(value.to_ascii_lowercase())
                }
                "kill_user_processes" => probe.kill_user_processes = yes_no(value),
                "linger" => probe.linger = yes_no(value),
                _ => {}
            }
        }
        probe
    }

    pub(crate) fn is_supported(&self) -> bool {
        self.platform == "Linux x86_64"
    }

    /// Logind ends everything the user started once their last session closes,
    /// the keeper included, unless the user lingers.
    pub(crate) fn suggests_linger(&self) -> bool {
        self.kill_user_processes == Some(true) && self.linger != Some(true)
    }
}

fn yes_no(value: &str) -> Option<bool> {
    match value {
        "yes" => Some(true),
        "no" => Some(false),
        _ => None,
    }
}

/// The installer's one-word verdict.
pub(crate) fn install_verdict(stdout: &[u8]) -> Result<(), &'static str> {
    match last_line(stdout) {
        Some("installed") => Ok(()),
        Some("checksum_mismatch") => Err("checksum_mismatch"),
        Some("checksum_tool_missing") => Err("checksum_tool_missing"),
        _ => Err("install_failed"),
    }
}

/// The JSON line `keep start` and `keep stop` print.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct KeeperReport {
    pub(crate) outcome: String,
    pub(crate) keeper: Option<String>,
    pub(crate) endpoint_url: Option<String>,
    pub(crate) auth_token: Option<String>,
    pub(crate) app_version: Option<String>,
    pub(crate) protocol_version: Option<String>,
    pub(crate) error: Option<String>,
}

impl KeeperReport {
    pub(crate) fn parse(stdout: &[u8]) -> Option<Self> {
        serde_json::from_str(last_line(stdout)?).ok()
    }

    /// The keeper's error class, or a fixed word when another build printed
    /// something that is not a class and so must not reach the logs.
    pub(crate) fn error_class(&self) -> String {
        match self.error.as_deref() {
            Some(class) if is_class(class) => class.to_string(),
            _ => "unrecognized".to_string(),
        }
    }
}

fn is_class(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 48
        && text
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
}

fn last_line(stdout: &[u8]) -> Option<&str> {
    std::str::from_utf8(stdout)
        .ok()?
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
}

/// The remote endpoint split so that only its port crosses SSH. The App Server
/// listens on loopback; any other host would make this App Shell forward to a
/// machine the user did not choose, so it is refused.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RemoteEndpoint {
    pub(crate) port: u16,
    pub(crate) path: String,
}

impl RemoteEndpoint {
    pub(crate) fn parse(url: &str) -> Option<Self> {
        let rest = url.strip_prefix("http://")?;
        let (authority, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
        let (host, port) = authority.rsplit_once(':')?;
        if !matches!(host, "127.0.0.1" | "localhost" | "[::1]") {
            return None;
        }
        Some(Self {
            port: port.parse().ok().filter(|port| *port != 0)?,
            path: if path.is_empty() { "/" } else { path }.to_string(),
        })
    }
}

/// Release versions and checksums are interpolated as shell words; refusing
/// anything else keeps them inert even if quoting were wrong.
pub(crate) fn is_version(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 64
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
}

pub(crate) fn is_sha256(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
#[path = "remote_tests.rs"]
mod tests;
