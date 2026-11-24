use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

#[path = "build_support/embedded_adapters.rs"]
mod embedded_adapters;

fn main() {
    embed_adapters();
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

/// Embeds the adapters `npm run build:adapters` staged (ADR-0079). A missing or
/// stale stage fails the build with the command that fixes it.
fn embed_adapters() {
    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("Cargo sets CARGO_MANIFEST_DIR"));
    let repository_root = manifest_dir.join("../..");
    println!("cargo:rerun-if-changed=build_support/embedded_adapters.rs");
    let generated = match embedded_adapters::generate(&repository_root) {
        Ok(generated) => generated,
        Err(message) => {
            // Keep watching the stage so producing it reruns this script.
            println!(
                "cargo:rerun-if-changed={}",
                repository_root.join("adapters/build").display()
            );
            panic!("\n\nOpenAIDE App Server build: {message}\n\n");
        }
    };
    for input in &generated.inputs {
        println!("cargo:rerun-if-changed={}", input.display());
    }
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"));
    fs::write(out_dir.join("embedded_adapters.rs"), generated.code)
        .expect("write the embedded adapter table");
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
