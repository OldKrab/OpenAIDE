use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    println!("cargo:rerun-if-env-changed=OPENAIDE_BUILD_COMMIT");
    watch_git_revision();

    let commit = env::var("OPENAIDE_BUILD_COMMIT")
        .ok()
        .and_then(valid_commit)
        .or_else(read_git_commit)
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=OPENAIDE_BUILD_COMMIT={commit}");

    embed_windows_metadata();
}

#[cfg(windows)]
fn embed_windows_metadata() {
    // SignPath validates these resources on the executable it signs. Keep the
    // version Cargo-owned so release stamping supplies the same version used by
    // the Desktop shell and installer.
    tauri_winres::WindowsResource::new()
        .compile()
        .expect("failed to embed OpenAIDE App Server Windows metadata");
}

#[cfg(not(windows))]
fn embed_windows_metadata() {}

fn watch_git_revision() {
    let Some(manifest_dir) = env::var_os("CARGO_MANIFEST_DIR").map(PathBuf::from) else {
        return;
    };
    let Some(git_dir) = git_path(&manifest_dir) else {
        return;
    };
    let head = git_dir.join("HEAD");
    println!("cargo:rerun-if-changed={}", head.display());
    println!(
        "cargo:rerun-if-changed={}",
        git_dir.join("packed-refs").display()
    );
    let Ok(head_contents) = fs::read_to_string(&head) else {
        return;
    };
    let Some(reference) = head_contents.strip_prefix("ref: ").map(str::trim) else {
        return;
    };
    println!(
        "cargo:rerun-if-changed={}",
        git_dir.join(reference).display()
    );
}

fn read_git_commit() -> Option<String> {
    let manifest_dir = env::var_os("CARGO_MANIFEST_DIR").map(PathBuf::from)?;
    git_output(&manifest_dir, &["rev-parse", "--verify", "HEAD"]).and_then(valid_commit)
}

fn git_path(manifest_dir: &Path) -> Option<PathBuf> {
    let path = PathBuf::from(git_output(manifest_dir, &["rev-parse", "--git-dir"])?);
    Some(if path.is_absolute() {
        path
    } else {
        manifest_dir.join(path)
    })
}

fn git_output(manifest_dir: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(manifest_dir)
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn valid_commit(value: String) -> Option<String> {
    let value = value.trim();
    (!value.is_empty() && value.len() <= 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| value.to_string())
}
