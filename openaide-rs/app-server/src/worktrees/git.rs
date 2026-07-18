use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use openaide_app_server_protocol::worktree::{WorktreeBaseSnapshot, WorktreeHead};

use super::WorktreeBase;
use crate::protocol::errors::RuntimeError;

pub(super) struct ResolvedBase {
    pub revision: String,
    pub commit: String,
}

#[derive(Debug)]
pub(super) struct GitRepositoryDiscovery {
    pub common_dir: PathBuf,
    pub project_root: PathBuf,
    pub worktrees: Vec<GitWorktree>,
    pub bases: Vec<WorktreeBaseSnapshot>,
}

#[derive(Debug)]
pub(super) struct GitWorktree {
    pub path: PathBuf,
    pub is_main: bool,
    pub head: WorktreeHead,
    pub locked_reason: Option<String>,
    pub prunable_reason: Option<String>,
}

pub(super) fn discover_project_repository(
    project_root: &Path,
) -> Result<Option<GitRepositoryDiscovery>, RuntimeError> {
    let canonical_project = match std::fs::canonicalize(project_root) {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(RuntimeError::NotReady(
                "Project root is unavailable".to_string(),
            ));
        }
        Err(error) => return Err(error.into()),
    };
    let Some(top_level) = try_git_text(project_root, &["rev-parse", "--show-toplevel"])? else {
        return Ok(None);
    };
    let canonical_top = std::fs::canonicalize(top_level.trim())?;
    if canonical_top != canonical_project {
        return Ok(None);
    }
    let common_dir = git_text(
        project_root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let common_dir = std::fs::canonicalize(common_dir.trim())?;
    let worktrees = parse_worktree_porcelain(&git_bytes(
        project_root,
        &["worktree", "list", "--porcelain", "-z"],
    )?)?;
    let bases = local_bases(project_root)?;
    Ok(Some(GitRepositoryDiscovery {
        common_dir,
        project_root: canonical_project,
        worktrees,
        bases,
    }))
}

pub(super) fn resolve_base(
    project_root: &Path,
    base: &WorktreeBase,
) -> Result<ResolvedBase, RuntimeError> {
    let revision = match base {
        WorktreeBase::CurrentHead => "HEAD".to_string(),
        WorktreeBase::LocalBranch(name) => {
            if name.trim().is_empty() {
                return Err(RuntimeError::InvalidParams(
                    "Base branch is required".to_string(),
                ));
            }
            format!("refs/heads/{name}")
        }
    };
    let commit = git_text(project_root, &["rev-parse", "--verify", &revision])?
        .trim()
        .to_string();
    Ok(ResolvedBase { revision, commit })
}

pub(super) fn validate_new_branch(project_root: &Path, branch: &str) -> Result<(), RuntimeError> {
    if branch.trim() != branch || branch.is_empty() {
        return Err(RuntimeError::InvalidParams(
            "Branch name is invalid".to_string(),
        ));
    }
    let validation = run_git(project_root, &["check-ref-format", "--branch", branch])?;
    if !validation.status.success() {
        return Err(RuntimeError::InvalidParams(
            "Branch name is invalid".to_string(),
        ));
    }
    let reference = format!("refs/heads/{branch}");
    if run_git(
        project_root,
        &["show-ref", "--verify", "--quiet", &reference],
    )?
    .status
    .success()
    {
        return Err(RuntimeError::Conflict(format!(
            "Branch already exists: {branch}"
        )));
    }
    Ok(())
}

pub(super) fn add_worktree(
    project_root: &Path,
    destination: &Path,
    revision: &str,
    branch: Option<&str>,
) -> Result<(), RuntimeError> {
    let destination = destination.to_string_lossy();
    let mut args = vec!["worktree", "add"];
    if let Some(branch) = branch {
        args.extend(["-b", branch]);
    } else {
        args.push("--detach");
    }
    args.extend([destination.as_ref(), revision]);
    git_bytes(project_root, &args).map(|_| ())
}

pub(super) fn add_recreated_worktree(
    project_root: &Path,
    destination: &Path,
    base_revision: &str,
    branch: Option<&str>,
) -> Result<(), RuntimeError> {
    let destination = destination.to_string_lossy();
    let mut args = vec!["worktree", "add"];
    match branch {
        Some(branch) => {
            validate_branch_format(project_root, branch)?;
            if branch_exists(project_root, branch)? {
                args.extend([destination.as_ref(), branch]);
            } else {
                args.extend(["-b", branch, destination.as_ref(), base_revision]);
            }
        }
        None => args.extend(["--detach", destination.as_ref(), base_revision]),
    }
    git_bytes(project_root, &args).map(|_| ())
}

pub(super) fn worktree_is_registered(
    project_root: &Path,
    destination: &Path,
) -> Result<bool, RuntimeError> {
    let discovered = parse_worktree_porcelain(&git_bytes(
        project_root,
        &["worktree", "list", "--porcelain", "-z"],
    )?)?;
    Ok(discovered
        .iter()
        .any(|worktree| worktree.path == destination))
}

pub(super) fn working_tree_dirty(worktree: &Path) -> Result<bool, RuntimeError> {
    Ok(!git_bytes(
        worktree,
        &[
            "status",
            "--porcelain=v2",
            "-z",
            "--untracked-files=all",
            "--ignore-submodules=none",
        ],
    )?
    .is_empty())
}

pub(super) fn has_initialized_submodules(worktree: &Path) -> Result<bool, RuntimeError> {
    let output = git_bytes(worktree, &["submodule", "status", "--recursive"])?;
    Ok(String::from_utf8_lossy(&output)
        .lines()
        .any(|line| !line.starts_with('-') && !line.trim().is_empty()))
}

pub(super) fn detached_head_is_preserved(
    worktree: &Path,
    head: &WorktreeHead,
) -> Result<bool, RuntimeError> {
    let WorktreeHead::Detached { commit } = head else {
        return Ok(true);
    };
    let contains = format!("--contains={commit}");
    Ok(!git_bytes(
        worktree,
        &[
            "for-each-ref",
            "--format=%(refname)",
            &contains,
            "refs/heads",
            "refs/tags",
        ],
    )?
    .is_empty())
}

pub(super) fn remove_worktree(git_cwd: &Path, worktree: &Path) -> Result<(), RuntimeError> {
    let worktree = worktree.to_string_lossy();
    git_bytes(
        git_cwd,
        &["worktree", "remove", "--force", worktree.as_ref()],
    )
    .map(|_| ())
}

fn validate_branch_format(project_root: &Path, branch: &str) -> Result<(), RuntimeError> {
    if branch.trim() != branch || branch.is_empty() {
        return Err(RuntimeError::InvalidParams(
            "Branch name is invalid".to_string(),
        ));
    }
    let validation = run_git(project_root, &["check-ref-format", "--branch", branch])?;
    if validation.status.success() {
        Ok(())
    } else {
        Err(RuntimeError::InvalidParams(
            "Branch name is invalid".to_string(),
        ))
    }
}

fn branch_exists(project_root: &Path, branch: &str) -> Result<bool, RuntimeError> {
    let reference = format!("refs/heads/{branch}");
    Ok(run_git(
        project_root,
        &["show-ref", "--verify", "--quiet", &reference],
    )?
    .status
    .success())
}

fn parse_worktree_porcelain(bytes: &[u8]) -> Result<Vec<GitWorktree>, RuntimeError> {
    let mut records = Vec::new();
    let mut fields = Vec::<String>::new();
    for field in bytes.split(|byte| *byte == 0) {
        if field.is_empty() {
            if !fields.is_empty() {
                records.push(parse_worktree_record(&fields, records.is_empty())?);
                fields.clear();
            }
            continue;
        }
        fields.push(String::from_utf8(field.to_vec()).map_err(|_| {
            RuntimeError::Internal("Git returned a non-UTF-8 worktree record".to_string())
        })?);
    }
    if !fields.is_empty() {
        records.push(parse_worktree_record(&fields, records.is_empty())?);
    }
    Ok(records)
}

fn parse_worktree_record(fields: &[String], is_main: bool) -> Result<GitWorktree, RuntimeError> {
    let path = field_value(fields, "worktree ")
        .map(PathBuf::from)
        .ok_or_else(|| RuntimeError::Internal("Git worktree record has no path".to_string()))?;
    let commit = field_value(fields, "HEAD ")
        .ok_or_else(|| RuntimeError::Internal("Git worktree record has no HEAD".to_string()))?
        .to_string();
    let head = match field_value(fields, "branch ") {
        Some(branch) => WorktreeHead::Branch {
            name: branch
                .strip_prefix("refs/heads/")
                .unwrap_or(branch)
                .to_string(),
            commit,
        },
        None => WorktreeHead::Detached { commit },
    };
    Ok(GitWorktree {
        path,
        is_main,
        head,
        locked_reason: optional_flag_reason(fields, "locked"),
        prunable_reason: optional_flag_reason(fields, "prunable"),
    })
}

fn field_value<'a>(fields: &'a [String], prefix: &str) -> Option<&'a str> {
    fields.iter().find_map(|field| field.strip_prefix(prefix))
}

fn optional_flag_reason(fields: &[String], flag: &str) -> Option<String> {
    fields.iter().find_map(|field| {
        if field == flag {
            Some(String::new())
        } else {
            field.strip_prefix(&format!("{flag} ")).map(str::to_string)
        }
    })
}

fn local_bases(project_root: &Path) -> Result<Vec<WorktreeBaseSnapshot>, RuntimeError> {
    let head = git_text(project_root, &["rev-parse", "HEAD"])?;
    let mut bases = vec![WorktreeBaseSnapshot::Head {
        commit: head.trim().to_string(),
        label: "Current HEAD".to_string(),
    }];
    let fields = git_bytes(
        project_root,
        &[
            "for-each-ref",
            "--format=%(refname:short)%00%(objectname)%00",
            "refs/heads",
        ],
    )?;
    let values = fields
        .split(|byte| *byte == 0 || *byte == b'\n')
        .filter(|value| !value.is_empty())
        .map(|value| String::from_utf8_lossy(value).to_string())
        .collect::<Vec<_>>();
    for pair in values.chunks_exact(2) {
        bases.push(WorktreeBaseSnapshot::LocalBranch {
            name: pair[0].clone(),
            commit: pair[1].clone(),
        });
    }
    Ok(bases)
}

fn try_git_text(cwd: &Path, args: &[&str]) -> Result<Option<String>, RuntimeError> {
    let output = run_git(cwd, args)?;
    if !output.status.success() {
        return Ok(None);
    }
    String::from_utf8(output.stdout)
        .map(Some)
        .map_err(|_| RuntimeError::Internal("Git returned non-UTF-8 text".to_string()))
}

fn git_text(cwd: &Path, args: &[&str]) -> Result<String, RuntimeError> {
    String::from_utf8(git_bytes(cwd, args)?)
        .map_err(|_| RuntimeError::Internal("Git returned non-UTF-8 text".to_string()))
}

pub(super) fn git_bytes(cwd: &Path, args: &[&str]) -> Result<Vec<u8>, RuntimeError> {
    let output = run_git(cwd, args)?;
    if output.status.success() {
        return Ok(output.stdout);
    }
    Err(RuntimeError::Internal(format!(
        "git {} failed: {}",
        args.first().copied().unwrap_or("command"),
        String::from_utf8_lossy(&output.stderr).trim()
    )))
}

fn run_git(cwd: &Path, args: &[&str]) -> Result<Output, RuntimeError> {
    Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|error| RuntimeError::Internal(format!("Failed to start Git: {error}")))
}
