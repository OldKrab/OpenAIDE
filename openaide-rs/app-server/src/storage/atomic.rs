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
    let parent = if parent.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent
    };
    create_directory(parent)?;

    // Every invocation owns its temporary inode, including concurrent writers
    // whose destinations share a stem. Publish only after its contents are durable.
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    replace_file(&temporary.into_temp_path(), path)?;
    sync_directory(parent)
}

fn create_directory(path: &Path) -> Result<(), RuntimeError> {
    if path.is_dir() {
        return Ok(());
    }
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        create_directory(parent)?;
    }
    match std::fs::create_dir(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && path.is_dir() => {}
        Err(error) => return Err(error.into()),
    }
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        sync_directory(parent)?;
    }
    Ok(())
}

#[cfg(unix)]
pub(crate) fn sync_directory(path: &Path) -> Result<(), RuntimeError> {
    std::fs::File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(windows)]
pub(crate) fn sync_directory(_path: &Path) -> Result<(), RuntimeError> {
    // Windows has no portable directory fsync. The replacement file was flushed
    // before the write-through replacement; opening directories fails there.
    Ok(())
}

#[cfg(all(not(unix), not(windows)))]
pub(crate) fn sync_directory(_path: &Path) -> Result<(), RuntimeError> {
    Err(RuntimeError::Storage(
        "Directory sync is unsupported on this platform".to_string(),
    ))
}

#[cfg(unix)]
pub(crate) fn replace_file(temporary: &Path, path: &Path) -> Result<(), RuntimeError> {
    std::fs::rename(temporary, path)?;
    Ok(())
}

#[cfg(windows)]
pub(crate) fn replace_file(temporary: &Path, path: &Path) -> Result<(), RuntimeError> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let destination = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let source = temporary
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    // SAFETY: both owned UTF-16 buffers are NUL terminated and remain alive
    // through the call. Replacement is required after the initial write.
    if unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        return Err(RuntimeError::from(std::io::Error::last_os_error()));
    }
    Ok(())
}

#[cfg(all(not(unix), not(windows)))]
pub(crate) fn replace_file(_temporary: &Path, _path: &Path) -> Result<(), RuntimeError> {
    Err(RuntimeError::Storage(
        "Atomic replacement is unsupported on this platform".to_string(),
    ))
}

#[cfg(test)]
#[path = "atomic_tests.rs"]
mod tests;
