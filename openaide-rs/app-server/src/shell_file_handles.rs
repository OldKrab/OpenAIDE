use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use openaide_app_server_protocol::ids::ClientInstanceId;
use uuid::Uuid;

use crate::protocol::errors::RuntimeError;

const MAX_ACTIVE_REVEAL_HANDLES: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellFileRevealHandle {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellFileRevealTarget {
    pub path: PathBuf,
    pub label: String,
    owner_client_instance_id: ClientInstanceId,
}

#[derive(Clone, Default)]
pub struct ShellFileRevealRegistry {
    inner: Arc<Mutex<ShellFileRevealRegistryInner>>,
}

#[derive(Default)]
struct ShellFileRevealRegistryInner {
    targets: HashMap<String, ShellFileRevealTarget>,
    registration_order: VecDeque<String>,
}

impl ShellFileRevealRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_local_file_for_client(
        &self,
        client_instance_id: ClientInstanceId,
        path: PathBuf,
        label: Option<String>,
    ) -> Result<ShellFileRevealHandle, RuntimeError> {
        if !path.is_absolute() {
            return Err(RuntimeError::InvalidParams("file path".to_string()));
        }
        let label = label.unwrap_or_else(|| safe_file_label(&path));
        let mut inner = self.inner.lock().expect("shell file registry poisoned");
        let id = format!("file-reveal-{}", Uuid::new_v4());
        inner.targets.insert(
            id.clone(),
            ShellFileRevealTarget {
                path,
                label: label.clone(),
                owner_client_instance_id: client_instance_id,
            },
        );
        inner.registration_order.push_back(id.clone());
        while inner.targets.len() > MAX_ACTIVE_REVEAL_HANDLES {
            if let Some(oldest_id) = inner.registration_order.pop_front() {
                inner.targets.remove(&oldest_id);
            }
        }
        Ok(ShellFileRevealHandle { id, label })
    }

    pub fn consume_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        file_handle_id: &str,
    ) -> Option<ShellFileRevealTarget> {
        let mut inner = self.inner.lock().expect("shell file registry poisoned");
        let owned = inner
            .targets
            .get(file_handle_id)
            .map(|target| &target.owner_client_instance_id)
            == Some(client_instance_id);
        if !owned {
            return None;
        }
        inner
            .registration_order
            .retain(|registered_id| registered_id != file_handle_id);
        inner.targets.remove(file_handle_id)
    }
}

fn safe_file_label(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("File")
        .to_string()
}

#[cfg(test)]
#[path = "shell_file_handles_tests.rs"]
mod tests;
