//! The product-pinned ACP adapters App Server installs into per-user storage and
//! launches with Node.js. Each owns one exact package version and the npm lockfile
//! of its dependency graph; locking, publication, leases, and pruning are shared.
//!
//! Codex is still an npm package App Server installs and patches. Claude is
//! embedded from its fork submodule (ADR-0079): npm installs only its dependencies
//! and the provisioner unpacks the adapter itself.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use super::embedded::{self, EmbeddedAdapter};
use super::runtime_patch;

// Workflows update each package pin together with its lockfile.
const CODEX_PACKAGE_JSON: &str = include_str!("../../../assets/codex-acp-runtime/package.json");
const CODEX_PACKAGE_LOCK: &str =
    include_str!("../../../assets/codex-acp-runtime/package-lock.json");
// Generated from the Claude fork submodule by `npm run adapters:lock`; the shared
// catalog and Android read the adapter identity from the same file, and build.rs
// refuses an embedded adapter whose commit differs from the one it records.
const CLAUDE_PACKAGE_JSON: &str = include_str!(
    "../../../../../packages/app-shell-contracts/src/agentCatalog/claude-acp/package.json"
);
const CLAUDE_PACKAGE_LOCK: &str = include_str!(
    "../../../../../packages/app-shell-contracts/src/agentCatalog/claude-acp/package-lock.json"
);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ManagedAcpRuntime {
    Codex,
    Claude,
}

/// The installed package identity. `runtime_id` names the immutable cache directory.
pub(crate) struct ManagedPackage {
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) runtime_id: String,
}

impl ManagedAcpRuntime {
    pub(crate) const ALL: [Self; 2] = [Self::Codex, Self::Claude];

    pub(crate) fn for_agent(agent_id: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|runtime| runtime.agent_id() == agent_id)
    }

    pub(crate) fn agent_id(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude-code",
        }
    }

    /// The product name used in setup errors.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::Claude => "Claude",
        }
    }

    /// The adapter compiled into App Server, for runtimes that do not install
    /// their adapter from npm.
    // TODO(ADR-0079 PR 2): embed Codex from its fork submodule too, so every built-in
    // shares one install path and the npm pin and `npx -y` declaration form can go.
    pub(super) fn embedded(self) -> Option<&'static EmbeddedAdapter> {
        match self {
            Self::Codex => None,
            Self::Claude => Some(embedded::adapter("claude-agent-acp")),
        }
    }

    /// The package the built-in declares; it identifies the locked graph before
    /// the provisioner resolves it to an installation. An embedded adapter has no
    /// npm release, so its fork commit rides as semver build metadata:
    /// `@openaide/claude-agent-acp@1.11.0+30726077a8be`.
    pub(crate) fn declared_spec(self) -> String {
        match self.embedded() {
            Some(adapter) => format!(
                "{}@{}+{}",
                adapter.name,
                adapter.version,
                adapter.short_commit()
            ),
            None => format!(
                "{}@{}",
                CODEX_PACKAGE_NAME,
                pinned_version(CODEX_PACKAGE_JSON, CODEX_PACKAGE_NAME)
            ),
        }
    }

    /// The launcher and arguments of the built-in declaration. An npm-installed
    /// adapter keeps the `npx -y` form, which a runtime without the provisioner can
    /// still launch. An embedded adapter has nothing for npx to fetch, so it
    /// declares `node <spec>`, which only the provisioner can resolve to an
    /// entrypoint; launched unresolved it fails visibly instead of running other code.
    pub(crate) fn declared_launch(self) -> (&'static str, Vec<String>) {
        match self.embedded() {
            Some(_) => ("node", vec![self.declared_spec()]),
            None => ("npx", vec!["-y".to_string(), self.declared_spec()]),
        }
    }

    pub(super) fn runtimes_dir(self) -> &'static str {
        match self {
            Self::Codex => "codex-acp",
            Self::Claude => "claude-agent-acp",
        }
    }

    pub(super) fn package(self) -> &'static ManagedPackage {
        static CODEX: OnceLock<ManagedPackage> = OnceLock::new();
        static CLAUDE: OnceLock<ManagedPackage> = OnceLock::new();
        match self {
            // The Codex runtime is patched, so its cache revision comes from the
            // patch manifest rather than the upstream version alone.
            Self::Codex => CODEX.get_or_init(|| {
                let manifest = runtime_patch::manifest();
                ManagedPackage {
                    name: manifest.package_name.clone(),
                    version: manifest.package_version.clone(),
                    runtime_id: manifest.runtime_id.clone(),
                }
            }),
            // The fork commit identifies the embedded code and, through the
            // lockfile generated from it, the dependency graph.
            Self::Claude => CLAUDE.get_or_init(|| {
                let adapter = self.embedded().expect("Claude is embedded");
                ManagedPackage {
                    name: adapter.name.to_string(),
                    version: adapter.version.to_string(),
                    runtime_id: adapter.runtime_id(),
                }
            }),
        }
    }

    /// The `package.json` and `package-lock.json` npm installs from.
    pub(super) fn installer_files(self) -> (&'static str, &'static str) {
        match self {
            Self::Codex => (CODEX_PACKAGE_JSON, CODEX_PACKAGE_LOCK),
            Self::Claude => (CLAUDE_PACKAGE_JSON, CLAUDE_PACKAGE_LOCK),
        }
    }

    /// Bounds one npm install. Claude downloads its native runtime (about 300 MB).
    pub(super) fn install_timeout(self) -> Duration {
        match self {
            Self::Codex => Duration::from_secs(120),
            Self::Claude => Duration::from_secs(10 * 60),
        }
    }

    pub(super) fn has_runtime_patch(self) -> bool {
        self == Self::Codex
    }

    pub(super) fn event(self, stage: &str) -> String {
        let prefix = match self {
            Self::Codex => "codex_acp",
            Self::Claude => "claude_acp",
        };
        format!("{prefix}_provision_{stage}")
    }
}

/// Native Claude binaries npm may install for this host; the SDK spawns the first
/// present. Linux lists both libc flavors because npm selects the matching one.
/// Empty on hosts the SDK publishes no binary for.
pub(super) fn claude_native_binaries(version_root: &Path, windows: bool) -> Vec<PathBuf> {
    let os = match std::env::consts::OS {
        "linux" => "linux",
        "macos" => "darwin",
        "windows" => "win32",
        _ => return Vec::new(),
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        _ => return Vec::new(),
    };
    let base = format!("@anthropic-ai/claude-agent-sdk-{os}-{arch}");
    let packages = if os == "linux" {
        vec![base.clone(), format!("{base}-musl")]
    } else {
        vec![base]
    };
    let binary = if windows { "claude.exe" } else { "claude" };
    packages
        .into_iter()
        .map(|package| version_root.join("node_modules").join(package).join(binary))
        .collect()
}

/// Native Codex binary npm installs for this Unix host, at the path the
/// `@openai/codex` launcher resolves. None on hosts Codex publishes no binary for.
pub(super) fn unix_codex_binary(version_root: &Path) -> Option<PathBuf> {
    let (package, target) = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => ("linux-x64", "x86_64-unknown-linux-musl"),
        ("linux", "aarch64") => ("linux-arm64", "aarch64-unknown-linux-musl"),
        ("macos", "x86_64") => ("darwin-x64", "x86_64-apple-darwin"),
        ("macos", "aarch64") => ("darwin-arm64", "aarch64-apple-darwin"),
        _ => return None,
    };
    Some(
        version_root
            .join("node_modules/@openai")
            .join(format!("codex-{package}"))
            .join("vendor")
            .join(target)
            .join("bin/codex"),
    )
}

const CODEX_PACKAGE_NAME: &str = "@openaide/codex-acp";

fn pinned_version(package_json: &str, package_name: &str) -> String {
    let policy: serde_json::Value =
        serde_json::from_str(package_json).expect("embedded ACP policy is valid JSON");
    policy["dependencies"][package_name]
        .as_str()
        .expect("embedded ACP policy pins its package version")
        .to_string()
}
