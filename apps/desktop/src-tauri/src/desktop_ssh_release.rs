//! The Linux x64 App Server this Desktop installs on SSH Computers (ADR-0069).
//!
//! It must be exactly this Desktop's release. The Windows Desktop already
//! bundles that binary for WSL. The macOS Desktop downloads the published Linux
//! archive once, keeps the binary in its cache, and trusts it only when its
//! SHA-256 matches the one the release build compiled in.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use openaide_remote_client::ServerRelease;
use sha2::{Digest, Sha256};

/// Set by the release build from the Linux binary it publishes.
const BUILT_IN_SHA256: Option<&str> = option_env!("OPENAIDE_SSH_APP_SERVER_SHA256");
const RELEASES: &str = "https://github.com/OldKrab/OpenAIDE/releases/download";
const ARCHIVE_ENTRY: &str = "openaide-app-server/openaide-app-server";
const MAX_ARCHIVE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_BINARY_BYTES: u64 = 512 * 1024 * 1024;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);

pub(crate) struct ReleaseSource {
    pub(crate) release: ServerRelease,
    origin: Origin,
}

enum Origin {
    /// A binary already on this computer whose checksum was just computed.
    File(PathBuf),
    Download {
        url: String,
        cache: PathBuf,
        sha256: String,
    },
}

/// Finds this Desktop's release binary. Errors are stable classes.
pub(crate) fn resolve(
    app_version: &str,
    wsl_resource_binary: &Path,
    cache_root: &Path,
) -> Result<ReleaseSource, &'static str> {
    if let Some(source) = development_override(app_version)? {
        return Ok(source);
    }
    if cfg!(target_os = "windows") && wsl_resource_binary.is_file() {
        return local(app_version, wsl_resource_binary.to_path_buf());
    }
    let sha256 = BUILT_IN_SHA256.ok_or("unavailable_in_build")?;
    let release = ServerRelease::new(app_version, sha256).ok_or("unavailable_in_build")?;
    Ok(ReleaseSource {
        release,
        origin: Origin::Download {
            url: format!(
                "{RELEASES}/v{app_version}/openaide-app-server-linux-x64-{app_version}.tar.gz"
            ),
            cache: cache_root
                .join("ssh-app-server")
                .join(app_version)
                .join("openaide-app-server"),
            sha256: sha256.to_string(),
        },
    })
}

/// A development Desktop has no published release; it may name a Linux build
/// of the App Server and the version it reports.
fn development_override(app_version: &str) -> Result<Option<ReleaseSource>, &'static str> {
    if !cfg!(debug_assertions) {
        return Ok(None);
    }
    let Some(path) = std::env::var_os("OPENAIDE_DESKTOP_SSH_APP_SERVER") else {
        return Ok(None);
    };
    let version = std::env::var("OPENAIDE_DESKTOP_SSH_APP_SERVER_VERSION")
        .unwrap_or_else(|_| app_version.to_string());
    local(&version, PathBuf::from(path)).map(Some)
}

fn local(version: &str, path: PathBuf) -> Result<ReleaseSource, &'static str> {
    let sha256 = sha256_file(&path).map_err(|_| "release_unreadable")?;
    Ok(ReleaseSource {
        release: ServerRelease::new(version, &sha256).ok_or("unavailable_in_build")?,
        origin: Origin::File(path),
    })
}

impl ReleaseSource {
    /// Opens the binary, downloading it first if the cache lacks it. Blocks.
    pub(crate) fn open(&self) -> Result<Box<dyn Read>, &'static str> {
        match &self.origin {
            Origin::File(path) => File::open(path)
                .map(|file| Box::new(file) as Box<dyn Read>)
                .map_err(|_| "release_unreadable"),
            Origin::Download { url, cache, sha256 } => {
                if sha256_file(cache).ok().as_deref() != Some(sha256.as_str()) {
                    download(url, cache, sha256)?;
                }
                File::open(cache)
                    .map(|file| Box::new(file) as Box<dyn Read>)
                    .map_err(|_| "release_unreadable")
            }
        }
    }
}

fn download(url: &str, cache: &Path, sha256: &str) -> Result<(), &'static str> {
    let started = Instant::now();
    desktop_log!("desktop_ssh_release_download_started attempt_count=1");
    let result = tauri::async_runtime::block_on(fetch_archive(url))
        .and_then(|archive| extract_verified(&archive, cache, sha256));
    desktop_log!(
        "desktop_ssh_release_download_completed outcome={} error_class={} duration_ms={} attempt_count=1",
        if result.is_ok() { "success" } else { "failure" },
        result.err().unwrap_or("none"),
        started.elapsed().as_millis()
    );
    result
}

async fn fetch_archive(url: &str) -> Result<Vec<u8>, &'static str> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(DOWNLOAD_TIMEOUT)
        .build()
        .map_err(|_| "download_failed")?;
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|_| "download_failed")?;
    if !response.status().is_success() {
        return Err(if response.status() == reqwest::StatusCode::NOT_FOUND {
            "release_not_published"
        } else {
            "download_failed"
        });
    }
    let mut archive = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| "download_failed")? {
        if archive.len() as u64 + chunk.len() as u64 > MAX_ARCHIVE_BYTES {
            return Err("download_too_large");
        }
        archive.extend_from_slice(&chunk);
    }
    Ok(archive)
}

/// Unpacks the one binary into the cache and keeps it only if it is the release.
fn extract_verified(archive: &[u8], cache: &Path, sha256: &str) -> Result<(), &'static str> {
    let directory = cache.parent().ok_or("cache_unwritable")?;
    std::fs::create_dir_all(directory).map_err(|_| "cache_unwritable")?;
    let temporary = directory.join(format!(".download-{}", uuid::Uuid::new_v4().simple()));
    let result = (|| {
        let mut entries = tar::Archive::new(flate2::read::GzDecoder::new(archive));
        for entry in entries.entries().map_err(|_| "archive_invalid")? {
            let entry = entry.map_err(|_| "archive_invalid")?;
            let is_binary = entry
                .path()
                .map(|path| path == Path::new(ARCHIVE_ENTRY))
                .unwrap_or(false);
            if !is_binary {
                continue;
            }
            let mut file = File::create(&temporary).map_err(|_| "cache_unwritable")?;
            let digest = copy_hashing(entry.take(MAX_BINARY_BYTES), &mut file)
                .map_err(|_| "archive_invalid")?;
            file.sync_all().map_err(|_| "cache_unwritable")?;
            if digest != sha256 {
                return Err("checksum_mismatch");
            }
            return std::fs::rename(&temporary, cache).map_err(|_| "cache_unwritable");
        }
        Err("archive_invalid")
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    copy_hashing(File::open(path)?, &mut std::io::sink())
}

fn copy_hashing(mut source: impl Read, target: &mut impl Write) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 64 * 1024];
    loop {
        let read = source.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        target.write_all(&buffer[..read])?;
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

#[cfg(test)]
#[path = "desktop_ssh_release_tests.rs"]
mod tests;
