use std::fs;

use crate::protocol::errors::RuntimeError;
use crate::storage::{atomic, Store};

use super::journal;

pub(crate) struct MessageFilesBackup {
    messages: MaterializedHistoryBackup,
    journal: AppendFileBackup,
    meta: Option<Vec<u8>>,
}

enum MaterializedHistoryBackup {
    Absent,
    HardLink(std::path::PathBuf),
    Bytes(Vec<u8>),
}

struct AppendFileBackup {
    existed: bool,
    len: u64,
}

impl Store {
    /// Captures rollback state without copying the potentially large materialized history.
    pub(crate) fn backup_message_files(
        &self,
        task_id: &str,
    ) -> Result<MessageFilesBackup, RuntimeError> {
        let task_dir = self.task_dir(task_id)?;
        let messages_path = task_dir.join("messages.jsonl");
        let rollback_path = task_dir.join(".messages.rollback.jsonl");
        remove_optional_file(&rollback_path)?;
        let messages = if !messages_path.exists() {
            MaterializedHistoryBackup::Absent
        } else {
            match fs::hard_link(&messages_path, &rollback_path) {
                Ok(()) => MaterializedHistoryBackup::HardLink(rollback_path),
                Err(error) => {
                    #[cfg(test)]
                    self.inner
                        .message_file_read_count
                        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    crate::logging::warn(
                        "message_history_backup_link_failed",
                        serde_json::json!({
                            "task_id": task_id,
                            "error_kind": format!("{:?}", error.kind()),
                        }),
                    );
                    MaterializedHistoryBackup::Bytes(fs::read(&messages_path)?)
                }
            }
        };
        let journal_path = journal::path(&task_dir);
        Ok(MessageFilesBackup {
            messages,
            journal: AppendFileBackup {
                existed: journal_path.exists(),
                len: journal_path
                    .metadata()
                    .map(|metadata| metadata.len())
                    .unwrap_or_default(),
            },
            meta: read_optional_bytes(&task_dir.join("message_meta.json"))?,
        })
    }

    pub(crate) fn restore_message_files(
        &self,
        task_id: &str,
        backup: &MessageFilesBackup,
    ) -> Result<(), RuntimeError> {
        let task_dir = self.task_dir(task_id)?;
        restore_materialized_history(&task_dir.join("messages.jsonl"), &backup.messages)?;
        restore_append_file(&journal::path(&task_dir), &backup.journal)?;
        restore_optional_bytes(&task_dir.join("message_meta.json"), backup.meta.as_deref())?;
        self.invalidate_agent_message_cache(task_id);
        Ok(())
    }

    pub(crate) fn discard_message_files_backup(&self, backup: &MessageFilesBackup) {
        let MaterializedHistoryBackup::HardLink(path) = &backup.messages else {
            return;
        };
        if let Err(error) = remove_optional_file(path) {
            crate::logging::warn(
                "message_history_backup_cleanup_failed",
                serde_json::json!({ "error": error.to_string() }),
            );
        }
    }
}

fn read_optional_bytes(path: &std::path::Path) -> Result<Option<Vec<u8>>, RuntimeError> {
    if path.exists() {
        Ok(Some(fs::read(path)?))
    } else {
        Ok(None)
    }
}

fn restore_optional_bytes(
    path: &std::path::Path,
    bytes: Option<&[u8]>,
) -> Result<(), RuntimeError> {
    match bytes {
        Some(bytes) => atomic::write_bytes(path, bytes),
        None => remove_optional_file(path),
    }
}

fn restore_materialized_history(
    path: &std::path::Path,
    backup: &MaterializedHistoryBackup,
) -> Result<(), RuntimeError> {
    match backup {
        MaterializedHistoryBackup::Absent => remove_optional_file(path),
        MaterializedHistoryBackup::HardLink(rollback_path) => {
            remove_optional_file(path)?;
            fs::rename(rollback_path, path)?;
            Ok(())
        }
        MaterializedHistoryBackup::Bytes(bytes) => atomic::write_bytes(path, bytes),
    }
}

fn restore_append_file(
    path: &std::path::Path,
    backup: &AppendFileBackup,
) -> Result<(), RuntimeError> {
    if !backup.existed {
        return remove_optional_file(path);
    }
    let file = fs::File::options().write(true).open(path)?;
    file.set_len(backup.len)?;
    file.sync_all().ok();
    Ok(())
}

fn remove_optional_file(path: &std::path::Path) -> Result<(), RuntimeError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(RuntimeError::from(error)),
    }
}
