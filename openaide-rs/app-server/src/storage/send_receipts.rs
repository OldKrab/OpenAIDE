use std::collections::BTreeMap;
use std::fs;

use serde::{Deserialize, Serialize};

use crate::protocol::errors::RuntimeError;

use super::{atomic, Store};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct TaskSendReceipt {
    pub idempotency_key: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachment_handles: Vec<String>,
    pub user_message_id: String,
    pub turn_id: String,
    /// True only after both locally owned Chat rows reached durable storage.
    #[serde(default, skip_serializing_if = "is_false")]
    pub durable_chat_written: bool,
}

fn is_false(value: &bool) -> bool {
    !value
}

impl Store {
    pub(crate) fn read_send_receipt(
        &self,
        task_id: &str,
        idempotency_key: &str,
    ) -> Result<Option<TaskSendReceipt>, RuntimeError> {
        Ok(self.read_send_receipts(task_id)?.remove(idempotency_key))
    }

    pub(crate) fn write_send_receipt(
        &self,
        task_id: &str,
        receipt: TaskSendReceipt,
    ) -> Result<(), RuntimeError> {
        let mut receipts = self.read_send_receipts(task_id)?;
        receipts.insert(receipt.idempotency_key.clone(), receipt);
        atomic::write_json(&self.send_receipts_path(task_id)?, &receipts)
    }

    pub(crate) fn backup_send_receipts(
        &self,
        task_id: &str,
    ) -> Result<Option<Vec<u8>>, RuntimeError> {
        let path = self.send_receipts_path(task_id)?;
        if path.exists() {
            Ok(Some(fs::read(path)?))
        } else {
            Ok(None)
        }
    }

    pub(crate) fn restore_send_receipts(
        &self,
        task_id: &str,
        backup: Option<&[u8]>,
    ) -> Result<(), RuntimeError> {
        let path = self.send_receipts_path(task_id)?;
        match backup {
            Some(bytes) => atomic::write_bytes(&path, bytes),
            None => match fs::remove_file(path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(RuntimeError::from(error)),
            },
        }
    }

    fn read_send_receipts(
        &self,
        task_id: &str,
    ) -> Result<BTreeMap<String, TaskSendReceipt>, RuntimeError> {
        let path = self.send_receipts_path(task_id)?;
        if !path.exists() {
            return Ok(BTreeMap::new());
        }
        let text = fs::read_to_string(path)?;
        serde_json::from_str(&text).map_err(RuntimeError::from)
    }

    fn send_receipts_path(&self, task_id: &str) -> Result<std::path::PathBuf, RuntimeError> {
        Ok(self.task_dir(task_id)?.join("send_receipts.json"))
    }
}
