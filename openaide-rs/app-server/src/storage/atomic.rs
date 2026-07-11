use std::fs::File;
use std::io::Write;
use std::path::Path;

use crate::protocol::errors::RuntimeError;

pub fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), RuntimeError> {
    let bytes = serde_json::to_vec_pretty(value)?;
    write_bytes(path, &bytes)
}

pub fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), RuntimeError> {
    let parent = path
        .parent()
        .ok_or_else(|| RuntimeError::Storage("path has no parent".to_string()))?;
    std::fs::create_dir_all(parent)?;

    let tmp = path.with_extension("tmp");
    {
        let mut file = File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all().ok();
    }
    std::fs::rename(tmp, path)?;
    Ok(())
}
