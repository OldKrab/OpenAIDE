//! Generates the App Server's embedded adapter table from `adapters/build/` (ADR-0079).
//!
//! `npm run build:adapters` stages each fork's publishable files with a manifest of
//! their SHA-256 digests. This module only reads that directory: it never runs Node
//! or touches the network. It refuses missing, stale, or modified staged output, so
//! a binary always embeds exactly the adapter commit its runtime lockfile pins.
//! Shared by `build.rs` and the `embedded_adapters_build` integration test.

use std::fmt::Write as _;
use std::fs;
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

const REBUILD: &str = "Run `npm run build:adapters` before building the App Server.";

pub struct Generated {
    /// Rust source defining `EMBEDDED_ADAPTERS`.
    pub code: String,
    /// Inputs whose change must regenerate the table.
    pub inputs: Vec<PathBuf>,
}

pub fn generate(repository_root: &Path) -> Result<Generated, String> {
    let config_path = repository_root.join("adapters/adapters.json");
    let mut inputs = vec![config_path.clone()];
    let config = read_json(&config_path)?;
    let adapters = config["adapters"]
        .as_array()
        .ok_or("adapters/adapters.json must list adapters")?;
    let mut code = String::from("pub(super) static EMBEDDED_ADAPTERS: &[EmbeddedAdapter] = &[\n");
    for adapter in adapters {
        let id = string(adapter, "id", "adapters/adapters.json")?;
        let runtime_package = string(adapter, "runtimePackage", "adapters/adapters.json")?;
        let staged = repository_root.join("adapters/build").join(id);
        let manifest_path = staged.join("manifest.json");
        // Watch the manifest even when it is missing, so building the adapter reruns this.
        inputs.push(manifest_path.clone());
        if !manifest_path.is_file() {
            return Err(format!(
                "embedded adapter `{id}` is not built: adapters/build/{id}/manifest.json is missing. {REBUILD}"
            ));
        }
        let manifest = read_json(&manifest_path)?;
        let policy_path = repository_root.join(runtime_package).join("package.json");
        inputs.push(policy_path.clone());
        let policy = read_json(&policy_path)?;
        let pinned = &policy["openaideAdapter"];
        let source = format!("adapters/build/{id}/manifest.json");
        let name = string(&manifest, "name", &source)?;
        let version = string(&manifest, "version", &source)?;
        let commit = string(&manifest, "commit", &source)?;
        if commit.len() != 40 || !commit.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(format!(
                "{source} does not name a full fork commit. {REBUILD}"
            ));
        }
        if pinned["name"].as_str() != Some(name)
            || pinned["version"].as_str() != Some(version)
            || pinned["commit"].as_str() != Some(commit)
        {
            return Err(format!(
                "staged adapter `{id}` ({name}@{version}, {}) is not the one {runtime_package}/package.json pins. \
                 Run `npm run adapters:lock` if you moved the submodule, then `npm run build:adapters`.",
                &commit[..12],
            ));
        }
        let files = manifest["files"]
            .as_array()
            .ok_or_else(|| format!("{source} lists no files. {REBUILD}"))?;
        let _ = writeln!(
            code,
            "    EmbeddedAdapter {{ id: {id:?}, name: {name:?}, version: {version:?}, commit: {commit:?}, files: &["
        );
        for file in files {
            let relative = string(file, "path", &source)?;
            let expected = string(file, "sha256", &source)?;
            let path = staged_file(&staged.join("package"), relative)
                .ok_or_else(|| format!("{source} names an unsafe path {relative:?}"))?;
            let bytes = fs::read(&path).map_err(|_| {
                format!("staged file {relative} of adapter `{id}` is missing. {REBUILD}")
            })?;
            if hex(&Sha256::digest(&bytes)) != expected {
                return Err(format!(
                    "staged file {relative} of adapter `{id}` does not match its manifest digest. {REBUILD}"
                ));
            }
            let _ = writeln!(
                code,
                "        EmbeddedFile {{ path: {relative:?}, sha256: {expected:?}, bytes: include_bytes!({:?}) }},",
                path.display().to_string(),
            );
            inputs.push(path);
        }
        code.push_str("    ] },\n");
    }
    code.push_str("];\n");
    Ok(Generated { code, inputs })
}

/// A manifest path resolved under the staged package; rejects anything that could escape it.
fn staged_file(package: &Path, relative: &str) -> Option<PathBuf> {
    let path = Path::new(relative);
    let safe = !relative.is_empty()
        && !relative.contains('\\')
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)));
    safe.then(|| package.join(path))
}

fn read_json(path: &Path) -> Result<serde_json::Value, String> {
    let text = fs::read_to_string(path).map_err(|_| format!("{} is unreadable", path.display()))?;
    serde_json::from_str(&text).map_err(|_| format!("{} is not valid JSON", path.display()))
}

fn string<'a>(value: &'a serde_json::Value, field: &str, source: &str) -> Result<&'a str, String> {
    value[field]
        .as_str()
        .ok_or_else(|| format!("{source} has no `{field}`"))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
