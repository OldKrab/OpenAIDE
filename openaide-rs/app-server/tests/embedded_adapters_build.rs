//! The App Server build embeds only complete, current staged adapters (ADR-0079).
//! Exercises the generator `build.rs` runs against a fixture repository layout.

#[path = "../build_support/embedded_adapters.rs"]
mod embedded_adapters;

use std::fs;
use std::path::Path;

use sha2::{Digest, Sha256};

const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";
const INDEX: &[u8] = b"#!/usr/bin/env node\n";

fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn write(path: &Path, contents: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

/// A repository with one adapter whose runtime package pins `pinned_commit`; the
/// staged build, when present, is of `COMMIT`.
fn repository(staged: bool, pinned_commit: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    write(
        &root.path().join("adapters/adapters.json"),
        serde_json::json!({ "adapters": [{
            "id": "fixture-acp",
            "submodule": "adapters/fixture-acp",
            "runtimePackage": "runtime/fixture-acp",
            "build": ["npm", "run", "build"],
        }]})
        .to_string(),
    );
    write(
        &root.path().join("runtime/fixture-acp/package.json"),
        serde_json::json!({ "openaideAdapter": {
            "name": "@openaide/fixture-acp", "version": "1.2.3", "commit": pinned_commit,
        }})
        .to_string(),
    );
    if staged {
        let stage = root.path().join("adapters/build/fixture-acp");
        write(&stage.join("package/dist/index.js"), INDEX);
        write(
            &stage.join("manifest.json"),
            serde_json::json!({
                "schema": 1, "id": "fixture-acp", "name": "@openaide/fixture-acp",
                "version": "1.2.3", "commit": COMMIT,
                "files": [{ "path": "dist/index.js", "size": INDEX.len(), "sha256": hex(INDEX) }],
            })
            .to_string(),
        );
    }
    root
}

#[test]
fn current_staged_adapter_is_embedded_with_its_digests() {
    let root = repository(true, COMMIT);

    let generated = embedded_adapters::generate(root.path()).expect("current stage embeds");

    assert!(generated.code.contains("name: \"@openaide/fixture-acp\""));
    assert!(generated.code.contains(&format!("commit: \"{COMMIT}\"")));
    assert!(generated.code.contains(&hex(INDEX)));
    assert!(generated.code.contains("include_bytes!("));
    let index = root
        .path()
        .join("adapters/build/fixture-acp/package/dist/index.js");
    assert!(generated.inputs.contains(&index));
}

#[test]
fn missing_stage_names_the_command_that_builds_it() {
    let root = repository(false, COMMIT);

    let message = embedded_adapters::generate(root.path())
        .err()
        .expect("missing stage");

    assert!(message.contains("fixture-acp"), "{message}");
    assert!(message.contains("npm run build:adapters"), "{message}");
}

#[test]
fn modified_staged_file_is_refused() {
    let root = repository(true, COMMIT);
    fs::write(
        root.path()
            .join("adapters/build/fixture-acp/package/dist/index.js"),
        "tampered",
    )
    .unwrap();

    let message = embedded_adapters::generate(root.path())
        .err()
        .expect("modified stage");

    assert!(
        message.contains("does not match its manifest digest"),
        "{message}"
    );
    assert!(message.contains("npm run build:adapters"), "{message}");
}

#[test]
fn stage_of_another_commit_than_the_runtime_lockfile_pins_is_refused() {
    let root = repository(true, &"f".repeat(40));

    let message = embedded_adapters::generate(root.path())
        .err()
        .expect("stale stage");

    assert!(message.contains("npm run adapters:lock"), "{message}");
    assert!(message.contains("npm run build:adapters"), "{message}");
}

#[test]
fn manifest_path_outside_the_staged_package_is_refused() {
    let root = repository(true, COMMIT);
    let manifest = root.path().join("adapters/build/fixture-acp/manifest.json");
    let text = fs::read_to_string(&manifest).unwrap().replace(
        "dist/index.js",
        "../../../../runtime/fixture-acp/package.json",
    );
    fs::write(&manifest, text).unwrap();

    let message = embedded_adapters::generate(root.path())
        .err()
        .expect("escaping path");

    assert!(message.contains("unsafe path"), "{message}");
}
