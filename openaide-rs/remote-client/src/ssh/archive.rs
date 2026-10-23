//! The published Linux x64 App Server archive, as App Shells unpack it before
//! uploading the binary to an SSH Computer (ADR-0069).
//!
//! The archive holds one binary at [`ARCHIVE_ENTRY`]. Only a binary whose
//! SHA-256 is the one the App Shell was built with is kept; anything else is
//! deleted before it can be uploaded.

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

/// The release asset name for `version`.
pub fn asset_name(version: &str) -> String {
    format!("openaide-app-server-linux-x64-{version}.tar.gz")
}

const ARCHIVE_ENTRY: &str = "openaide-app-server/openaide-app-server";
const MAX_BINARY_BYTES: u64 = 512 * 1024 * 1024;

/// Unpacks the binary to `destination` and keeps it only if its SHA-256 is
/// `sha256`. Errors are stable classes.
pub fn extract_verified(
    archive: impl Read,
    destination: &Path,
    sha256: &str,
) -> Result<(), &'static str> {
    let directory = destination.parent().ok_or("cache_unwritable")?;
    std::fs::create_dir_all(directory).map_err(|_| "cache_unwritable")?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    let temporary = directory.join(format!(".download-{}-{nonce}", std::process::id()));
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
            return std::fs::rename(&temporary, destination).map_err(|_| "cache_unwritable");
        }
        Err("archive_invalid")
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

/// The SHA-256 of a file, as lowercase hex.
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
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
#[path = "archive_tests.rs"]
mod tests;
