//! The product-pinned ACP adapters App Server installs into per-user storage and
//! launches with Node.js. Each owns one exact package version and the npm lockfile
//! of its dependency graph; locking, publication, leases, and pruning are shared.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use super::runtime_patch;

// Workflows update each package pin together with its lockfile.
const CODEX_PACKAGE_JSON: &str = include_str!("../../../assets/codex-acp-runtime/package.json");
const CODEX_PACKAGE_LOCK: &str =
    include_str!("../../../assets/codex-acp-runtime/package-lock.json");
// The shared catalog and App Server read the same Claude pin.
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

    fn package_name(self) -> &'static str {
        match self {
            Self::Codex => "@openaide/codex-acp",
            Self::Claude => "@openaide/claude-agent-acp",
        }
    }

    /// The `name@version` the built-in declares; it identifies the locked graph
    /// before the provisioner resolves it to an installation.
    pub(crate) fn declared_spec(self) -> String {
        let (package_json, _) = self.installer_files();
        format!(
            "{}@{}",
            self.package_name(),
            pinned_version(package_json, self.package_name())
        )
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
            // The lockfile pins the whole graph, so the adapter version identifies it.
            Self::Claude => CLAUDE.get_or_init(|| {
                let version = pinned_version(CLAUDE_PACKAGE_JSON, self.package_name());
                ManagedPackage {
                    name: self.package_name().to_string(),
                    runtime_id: version.clone(),
                    version,
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

fn pinned_version(package_json: &str, package_name: &str) -> String {
    let policy: serde_json::Value =
        serde_json::from_str(package_json).expect("embedded ACP policy is valid JSON");
    policy["dependencies"][package_name]
        .as_str()
        .expect("embedded ACP policy pins its package version")
        .to_string()
}
