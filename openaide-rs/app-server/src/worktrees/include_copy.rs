use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::{Component, Path, PathBuf};

use uuid::Uuid;

use crate::protocol::errors::RuntimeError;

use super::git::git_bytes;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct CopyOutcome {
    pub copied_files: u64,
    pub copied_bytes: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct CopyProgress {
    pub completed_files: u64,
    pub total_files: u64,
    pub completed_bytes: u64,
    pub total_bytes: u64,
}

pub(super) fn copy_included_files(
    source_root: &Path,
    destination_root: &Path,
) -> Result<CopyOutcome, RuntimeError> {
    copy_included_files_with_progress(source_root, destination_root, |_| {})
}

/// Copies the effective include plan and reports processing progress without exposing paths.
pub(super) fn copy_included_files_with_progress(
    source_root: &Path,
    destination_root: &Path,
    mut report: impl FnMut(CopyProgress),
) -> Result<CopyOutcome, RuntimeError> {
    let include_file = source_root.join(".worktreeinclude");
    if !include_file.is_file() {
        report(CopyProgress::default());
        return Ok(CopyOutcome::default());
    }
    let ignored = git_paths(
        source_root,
        &["ls-files", "-o", "-i", "--exclude-standard", "-z"],
    )?;
    let included = git_paths(
        source_root,
        &[
            "ls-files",
            "-o",
            "-i",
            "-z",
            "-X",
            include_file.to_string_lossy().as_ref(),
        ],
    )?;
    let included = included.into_iter().collect::<HashSet<_>>();
    let plan = ignored
        .into_iter()
        .filter(|relative| included.contains(relative))
        .collect::<Vec<_>>();
    let total_bytes = plan
        .iter()
        .filter_map(|relative| fs::symlink_metadata(source_root.join(relative)).ok())
        .filter(|metadata| metadata.file_type().is_file())
        .map(|metadata| metadata.len())
        .sum();
    let mut progress = CopyProgress {
        total_files: plan.len() as u64,
        total_bytes,
        ..CopyProgress::default()
    };
    report(progress);
    let mut outcome = CopyOutcome::default();
    let mut errors = Vec::new();
    for relative in plan {
        let planned_bytes = fs::symlink_metadata(source_root.join(&relative))
            .ok()
            .filter(|metadata| metadata.file_type().is_file())
            .map_or(0, |metadata| metadata.len());
        match copy_one(source_root, destination_root, &relative) {
            Ok(Some(bytes)) => {
                outcome.copied_files += 1;
                outcome.copied_bytes += bytes;
            }
            Ok(None) => {}
            Err(error) => errors.push(error.to_string()),
        }
        progress.completed_files = progress.completed_files.saturating_add(1);
        progress.completed_bytes = progress.completed_bytes.saturating_add(planned_bytes);
        report(progress);
    }
    if errors.is_empty() {
        Ok(outcome)
    } else {
        Err(RuntimeError::Storage(format!(
            "Worktree local-file copy failed for {} path(s): {}",
            errors.len(),
            errors.join("; ")
        )))
    }
}

fn git_paths(root: &Path, args: &[&str]) -> Result<Vec<PathBuf>, RuntimeError> {
    git_bytes(root, args)?
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| {
            String::from_utf8(path.to_vec())
                .map(PathBuf::from)
                .map_err(|_| RuntimeError::Storage("Git returned a non-UTF-8 path".to_string()))
        })
        .collect()
}

fn copy_one(
    source_root: &Path,
    destination_root: &Path,
    relative: &Path,
) -> Result<Option<u64>, RuntimeError> {
    validate_relative(relative)?;
    let source = source_root.join(relative);
    let destination = destination_root.join(relative);
    let metadata = fs::symlink_metadata(&source)?;
    if !metadata.file_type().is_file() {
        return Ok(None);
    }
    if destination.exists() {
        if files_equal(&source, &destination)? {
            return Ok(None);
        }
        return Err(RuntimeError::Conflict(format!(
            "Destination already contains {}",
            relative.to_string_lossy()
        )));
    }
    let parent = destination
        .parent()
        .ok_or_else(|| RuntimeError::Storage("Destination has no parent".to_string()))?;
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".openaide-copy-{}.tmp", Uuid::new_v4()));
    let copied = fs::copy(&source, &temporary)?;
    fs::set_permissions(&temporary, metadata.permissions())?;
    if let Err(error) = fs::rename(&temporary, &destination) {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    Ok(Some(copied))
}

fn validate_relative(path: &Path) -> Result<(), RuntimeError> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(RuntimeError::Storage(
            "Git returned a path outside the worktree root".to_string(),
        ));
    }
    Ok(())
}

fn files_equal(left: &Path, right: &Path) -> Result<bool, RuntimeError> {
    if fs::metadata(left)?.len() != fs::metadata(right)?.len() {
        return Ok(false);
    }
    let mut left = BufReader::new(File::open(left)?);
    let mut right = BufReader::new(File::open(right)?);
    let mut left_buffer = [0_u8; 8192];
    let mut right_buffer = [0_u8; 8192];
    loop {
        let left_read = left.read(&mut left_buffer)?;
        let right_read = right.read(&mut right_buffer)?;
        if left_read != right_read || left_buffer[..left_read] != right_buffer[..right_read] {
            return Ok(false);
        }
        if left_read == 0 {
            return Ok(true);
        }
    }
}
