use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::{npx_entry_name, npx_package_spec, repair_entry, NpxCacheRepairOutcome};

const CLAUDE_SPEC: &str = "@openaide/claude-agent-acp@1.7.0";

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

fn claude_spec() -> super::NpxPackageSpec {
    npx_package_spec("npx", &args(&["-y", CLAUDE_SPEC])).expect("pinned spec")
}

/// Writes an entry the way npm leaves it; `linked` controls whether the command link exists.
fn write_entry(cache: &Path, linked: bool) -> PathBuf {
    let entry = cache.join("_npx").join(npx_entry_name(&claude_spec()));
    let package = entry.join("node_modules/@openaide/claude-agent-acp");
    fs::create_dir_all(package.join("dist")).unwrap();
    fs::write(package.join("dist/index.js"), "").unwrap();
    fs::write(
        package.join("package.json"),
        r#"{"name":"@openaide/claude-agent-acp","bin":{"claude-agent-acp":"dist/index.js"}}"#,
    )
    .unwrap();
    let bin = entry.join("node_modules/.bin");
    fs::create_dir_all(&bin).unwrap();
    if linked {
        fs::write(bin.join("claude-agent-acp"), "").unwrap();
        fs::write(bin.join("claude-agent-acp.cmd"), "").unwrap();
    }
    entry
}

/// Evaluates as if the install finished long ago, past every in-progress guard.
fn later() -> SystemTime {
    // timing: data — an injected evaluation time past both in-progress windows; never waited on.
    SystemTime::now() + Duration::from_secs(10 * 60)
}

#[test]
fn entry_name_matches_npm_hash_of_the_spec() {
    // Observed name of the npm 11 entry for this exact spec.
    assert_eq!(npx_entry_name(&claude_spec()), "2819e235632ff562");
}

#[test]
fn only_exact_version_npx_packages_are_recognized() {
    assert!(npx_package_spec("/usr/bin/npx", &args(&["-y", CLAUDE_SPEC])).is_some());
    assert!(npx_package_spec("npx.cmd", &args(&["-y", "pkg@2.0.0-rc.1"])).is_some());
    assert!(npx_package_spec("npx", &args(&["-y", "opencode-ai", "acp"])).is_none());
    assert!(npx_package_spec("npx", &args(&["-y", "@scope/pkg"])).is_none());
    assert!(npx_package_spec("npx", &args(&["-y", "pkg@latest"])).is_none());
    assert!(npx_package_spec("node", &args(&["-y", CLAUDE_SPEC])).is_none());
    assert!(npx_package_spec("npx", &args(&[CLAUDE_SPEC])).is_none());
}

#[test]
fn entry_missing_its_command_is_removed() {
    let cache = tempfile::tempdir().unwrap();
    let entry = write_entry(cache.path(), false);

    let outcome = repair_entry(cache.path(), &claude_spec(), later());

    assert_eq!(outcome, NpxCacheRepairOutcome::Repaired);
    assert!(!entry.exists());
}

#[test]
fn complete_entry_is_kept() {
    let cache = tempfile::tempdir().unwrap();
    let entry = write_entry(cache.path(), true);

    let outcome = repair_entry(cache.path(), &claude_spec(), later());

    assert_eq!(outcome, NpxCacheRepairOutcome::Healthy);
    assert!(entry.exists());
}

#[test]
fn entry_held_by_a_live_npm_install_is_kept() {
    let cache = tempfile::tempdir().unwrap();
    let entry = write_entry(cache.path(), false);
    // npm's lock is a directory; a file carries the same mtime and is portable to set.
    let lock = entry.join("concurrency.lock");
    let now = later();
    fs::File::create(&lock)
        .unwrap()
        // timing: data — a lock refreshed within npm's stale window, relative to `now`.
        .set_modified(now - Duration::from_secs(5))
        .unwrap();

    let outcome = repair_entry(cache.path(), &claude_spec(), now);

    assert_eq!(outcome, NpxCacheRepairOutcome::InstallInProgress);
    assert!(entry.exists());
}

#[test]
fn entry_still_being_written_is_kept() {
    let cache = tempfile::tempdir().unwrap();
    let entry = write_entry(cache.path(), false);

    let outcome = repair_entry(cache.path(), &claude_spec(), SystemTime::now());

    assert_eq!(outcome, NpxCacheRepairOutcome::InstallInProgress);
    assert!(entry.exists());
}

#[test]
fn entry_without_the_extracted_package_is_left_to_npm() {
    let cache = tempfile::tempdir().unwrap();
    let entry = write_entry(cache.path(), false);
    fs::remove_dir_all(entry.join("node_modules/@openaide")).unwrap();

    let outcome = repair_entry(cache.path(), &claude_spec(), later());

    assert_eq!(outcome, NpxCacheRepairOutcome::PackageNotInstalled);
    assert!(entry.exists());
}

#[test]
fn absent_entry_and_cache_are_reported() {
    let cache = tempfile::tempdir().unwrap();

    assert_eq!(
        repair_entry(cache.path(), &claude_spec(), later()),
        NpxCacheRepairOutcome::EntryNotFound
    );
    assert_eq!(
        repair_entry(&cache.path().join("missing"), &claude_spec(), later()),
        NpxCacheRepairOutcome::CacheNotFound
    );
}
