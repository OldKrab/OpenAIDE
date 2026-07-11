use std::path::PathBuf;
use std::time::Instant;

use openaide_app_server_protocol::attachment::{
    AttachmentCreateFileReferenceResult, AttachmentListDirectoryResult, AttachmentListRootsResult,
    FileBrowserDirectory, FileBrowserEntry, FileBrowserEntryKind, FileBrowserRoot,
    PreSendAttachment,
};
use openaide_app_server_protocol::ids::{FileBrowserEntryId, FileBrowserRootId, TaskId};

use super::path_validation::{AllowedRoot, ValidatedPathKind};
use super::{AttachmentOwner, AttachmentRuntime, AttachmentRuntimeError, FileBrowserEntryHandle};

impl AttachmentRuntime {
    pub(crate) fn list_roots(
        &self,
        _task_id: &TaskId,
        workspace_root: impl Into<PathBuf>,
    ) -> AttachmentListRootsResult {
        let workspace_root = workspace_root.into();
        AttachmentListRootsResult {
            roots: vec![FileBrowserRoot {
                root_id: root_id(),
                label: safe_path_label(&workspace_root),
            }],
        }
    }

    pub(crate) fn list_directory(
        &self,
        owner: impl Into<AttachmentOwner>,
        workspace_root: impl Into<PathBuf>,
        requested_root_id: &FileBrowserRootId,
        directory_id: Option<&FileBrowserEntryId>,
    ) -> Result<AttachmentListDirectoryResult, AttachmentRuntimeError> {
        let owner = owner.into();
        if requested_root_id != &root_id() {
            return Err(AttachmentRuntimeError::InvalidRoot);
        }
        let workspace_root = workspace_root.into();
        let (directory_path, directory_label, allowed_root) =
            self.resolve_directory(&owner, &workspace_root, requested_root_id, directory_id)?;
        allowed_root.validate_directory(&directory_path)?;
        let discovered = discover_entries(&directory_path, &allowed_root)?;
        Ok(self.register_listing(
            &owner,
            requested_root_id,
            directory_id,
            directory_label,
            allowed_root,
            discovered,
        ))
    }

    pub(crate) fn create_file_reference(
        &self,
        owner: impl Into<AttachmentOwner>,
        entry_id: &FileBrowserEntryId,
    ) -> Result<AttachmentCreateFileReferenceResult, AttachmentRuntimeError> {
        let owner = owner.into();
        let entry = {
            let mut state = self
                .state
                .lock()
                .expect("attachment runtime mutex poisoned");
            state.prune_expired(Instant::now());
            state
                .entries
                .get(entry_id.as_str())
                .cloned()
                .ok_or(AttachmentRuntimeError::UnknownEntry)?
        };
        if !entry.owner.belongs_to(&owner) {
            return Err(if entry.owner.belongs_to_task(&owner) {
                AttachmentRuntimeError::UnknownEntry
            } else {
                AttachmentRuntimeError::WrongTask
            });
        }
        if entry.kind != FileBrowserEntryKind::File {
            return Err(AttachmentRuntimeError::NotFile);
        }
        entry.allowed_root.validate_file(&entry.path)?;
        let registered = self.register_file_reference(
            &owner,
            entry.label.clone(),
            entry.path,
            entry.allowed_root,
        );
        Ok(AttachmentCreateFileReferenceResult {
            attachment: PreSendAttachment {
                handle_id: registered.handle_id,
                label: registered.label,
            },
        })
    }

    fn resolve_directory(
        &self,
        owner: &AttachmentOwner,
        workspace_root: &std::path::Path,
        requested_root_id: &FileBrowserRootId,
        directory_id: Option<&FileBrowserEntryId>,
    ) -> Result<(PathBuf, String, AllowedRoot), AttachmentRuntimeError> {
        match directory_id {
            Some(entry_id) => {
                let mut state = self
                    .state
                    .lock()
                    .expect("attachment runtime mutex poisoned");
                state.prune_expired(Instant::now());
                let entry = state
                    .entries
                    .get(entry_id.as_str())
                    .ok_or(AttachmentRuntimeError::UnknownEntry)?;
                if !entry.owner.belongs_to(owner) {
                    return Err(if entry.owner.belongs_to_task(owner) {
                        AttachmentRuntimeError::UnknownEntry
                    } else {
                        AttachmentRuntimeError::WrongTask
                    });
                }
                if &entry.root_id != requested_root_id {
                    return Err(AttachmentRuntimeError::WrongTask);
                }
                if entry.kind != FileBrowserEntryKind::Directory {
                    return Err(AttachmentRuntimeError::NotDirectory);
                }
                let current_root = AllowedRoot::new(workspace_root)?;
                if entry.allowed_root != current_root {
                    return Err(AttachmentRuntimeError::InvalidRoot);
                }
                Ok((
                    entry.path.clone(),
                    entry.label.clone(),
                    entry.allowed_root.clone(),
                ))
            }
            None => Ok((
                workspace_root.to_path_buf(),
                safe_path_label(workspace_root),
                AllowedRoot::new(workspace_root)?,
            )),
        }
    }

    fn register_listing(
        &self,
        owner: &AttachmentOwner,
        requested_root_id: &FileBrowserRootId,
        directory_id: Option<&FileBrowserEntryId>,
        directory_label: String,
        allowed_root: AllowedRoot,
        discovered: Vec<DiscoveredEntry>,
    ) -> AttachmentListDirectoryResult {
        let mut state = self
            .state
            .lock()
            .expect("attachment runtime mutex poisoned");
        state.prune_expired(Instant::now());
        let mut entries = Vec::with_capacity(discovered.len());
        for item in discovered {
            state.next_entry_id += 1;
            let entry_id = FileBrowserEntryId::from(format!("file-entry-{}", state.next_entry_id));
            state.entries.insert(
                entry_id.as_str().to_string(),
                FileBrowserEntryHandle {
                    owner: owner.clone(),
                    root_id: requested_root_id.clone(),
                    label: item.label.clone(),
                    path: item.path,
                    allowed_root: allowed_root.clone(),
                    kind: item.kind,
                    expires_at: self.expires_at(),
                },
            );
            entries.push(FileBrowserEntry {
                entry_id,
                label: item.label,
                kind: item.kind,
                size_bytes: item.size_bytes,
                selectable: item.kind == FileBrowserEntryKind::File,
            });
        }
        AttachmentListDirectoryResult {
            directory: FileBrowserDirectory {
                label: directory_label,
                root_id: requested_root_id.clone(),
                directory_id: directory_id.cloned(),
            },
            entries,
        }
    }
}

#[derive(Debug)]
struct DiscoveredEntry {
    label: String,
    path: PathBuf,
    kind: FileBrowserEntryKind,
    size_bytes: Option<u64>,
}

fn discover_entries(
    directory_path: &std::path::Path,
    allowed_root: &AllowedRoot,
) -> Result<Vec<DiscoveredEntry>, AttachmentRuntimeError> {
    let read_dir = std::fs::read_dir(directory_path)
        .map_err(|error| AttachmentRuntimeError::ReadFailed(error.to_string()))?;
    let mut discovered = Vec::new();
    for item in read_dir {
        let item = item.map_err(|error| AttachmentRuntimeError::ReadFailed(error.to_string()))?;
        let label = item.file_name().to_string_lossy().to_string();
        let selected_path = item.path();
        let (kind, size_bytes) = match allowed_root.classify(&selected_path) {
            Ok(ValidatedPathKind::Directory) => (FileBrowserEntryKind::Directory, None),
            Ok(ValidatedPathKind::File { size_bytes }) => {
                (FileBrowserEntryKind::File, Some(size_bytes))
            }
            Err(_) => continue,
        };
        if kind == FileBrowserEntryKind::Directory && is_hidden_file_browser_directory(&label) {
            continue;
        }
        discovered.push(DiscoveredEntry {
            label,
            path: selected_path,
            kind,
            size_bytes,
        });
    }
    discovered.sort_by(|left, right| {
        left.kind
            .sort_key()
            .cmp(&right.kind.sort_key())
            .then_with(|| left.label.to_lowercase().cmp(&right.label.to_lowercase()))
            .then_with(|| left.label.cmp(&right.label))
    });
    Ok(discovered)
}

fn is_hidden_file_browser_directory(label: &str) -> bool {
    if label.starts_with('.') {
        return true;
    }
    matches!(
        label,
        "build"
            | "coverage"
            | "dist"
            | "node_modules"
            | "qa-artifacts"
            | "qa-scripts"
            | "target"
            | "test-results"
            | "tmp"
    )
}

trait FileBrowserEntryKindSort {
    fn sort_key(self) -> u8;
}

impl FileBrowserEntryKindSort for FileBrowserEntryKind {
    fn sort_key(self) -> u8 {
        match self {
            FileBrowserEntryKind::Directory => 0,
            FileBrowserEntryKind::File => 1,
        }
    }
}

fn root_id() -> FileBrowserRootId {
    FileBrowserRootId::from("task-root-1")
}

fn safe_path_label(path: &std::path::Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|label| !label.trim().is_empty())
        .unwrap_or_else(|| "Workspace".to_string())
}
