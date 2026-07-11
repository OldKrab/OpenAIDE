use std::path::{Component, Path, PathBuf};

use openaide_app_server_protocol::ids::ProjectId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectIdentity {
    pub project_id: ProjectId,
    pub workspace_root: String,
    pub label: String,
}

impl ProjectIdentity {
    pub fn from_workspace_root(workspace_root: &str) -> Self {
        let workspace_root = canonical_workspace_root(workspace_root);
        Self {
            project_id: project_id_from_canonical_root(&workspace_root),
            label: safe_project_label(&workspace_root),
            workspace_root,
        }
    }
}

pub fn project_id_for_workspace(workspace_root: &str) -> ProjectId {
    ProjectIdentity::from_workspace_root(workspace_root).project_id
}

fn project_id_from_canonical_root(workspace_root: &str) -> ProjectId {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in workspace_root.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    ProjectId::from(format!("project-{hash:016x}"))
}

fn canonical_workspace_root(workspace_root: &str) -> String {
    if workspace_root.is_empty() {
        return String::new();
    }

    let mut normalized = PathBuf::new();
    let absolute = Path::new(workspace_root).has_root();
    for component in Path::new(workspace_root).components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() && !absolute {
                    normalized.push("..");
                }
            }
            Component::Normal(segment) => normalized.push(segment),
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
        }
    }

    let normalized = normalized.to_string_lossy().to_string();
    if normalized.is_empty() {
        ".".to_string()
    } else {
        normalized
    }
}

fn safe_project_label(workspace_root: &str) -> String {
    Path::new(workspace_root)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("Project")
        .to_string()
}

#[cfg(test)]
mod tests;
