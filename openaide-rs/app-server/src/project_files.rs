//! Bounded, read-only workspace discovery. Authorization happens before this module;
//! canonical containment here prevents discovery from becoming arbitrary-path reads.
use openaide_app_server_protocol::project_files::*;
use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
    time::{Duration, Instant},
};
mod git;
#[cfg(test)]
mod tests;
const PAGE: usize = 200;
const SCAN: usize = 20_000;
const FILE_BYTES: u64 = 1024 * 1024;

pub fn run(root: &Path, operation: &str, params: &ProjectFilesParams) -> ProjectFilesResult {
    let mut result = empty();
    let root = match fs::canonicalize(root) {
        Ok(root) => root,
        Err(_) => {
            result.error = Some("unavailable".into());
            return result;
        }
    };
    let outcome = match operation {
        "fileViewer/listDirectory" => directory(&root, params, &mut result),
        "fileViewer/search" => search(&root, params, &mut result),
        "fileViewer/changes" => git::changes(&root, params, &mut result),
        "fileViewer/diff" => git::diff(&root, params, &mut result),
        _ => Err("unavailable"),
    };
    if let Err(error) = outcome {
        result.error = Some(error.into());
    }
    result
}
fn empty() -> ProjectFilesResult {
    ProjectFilesResult {
        entries: vec![],
        next_cursor: None,
        truncated: false,
        error: None,
        branch: None,
        base: None,
        diff: None,
    }
}
fn entry(path: String, directory: bool) -> ProjectFileEntry {
    ProjectFileEntry {
        name: path.rsplit('/').next().unwrap_or(&path).into(),
        path,
        directory,
        status: None,
        old_path: None,
        line: None,
        text: None,
    }
}
/// Missing leaf paths are allowed only for reviewing deletions; their existing parent
/// still has to resolve inside the workspace. Absolute and parent components are rejected.
fn resolve(root: &Path, relative: &str, missing: bool) -> Result<PathBuf, &'static str> {
    let path = Path::new(relative);
    if path
        .components()
        .any(|part| !matches!(part, Component::Normal(_) | Component::CurDir))
    {
        return Err("outsideWorkspace");
    }
    let joined = root.join(path);
    let mut check = joined.as_path();
    while missing && !check.exists() {
        check = check.parent().ok_or("outsideWorkspace")?;
    }
    let canonical = fs::canonicalize(check).map_err(|_| "unavailable")?;
    if !canonical.starts_with(root) {
        return Err("outsideWorkspace");
    }
    Ok(joined)
}
fn relative(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()?
        .to_str()
        .map(|s| s.replace(std::path::MAIN_SEPARATOR, "/"))
}
fn directory(
    root: &Path,
    params: &ProjectFilesParams,
    result: &mut ProjectFilesResult,
) -> Result<(), &'static str> {
    let path = resolve(root, &params.path, false)?;
    let started = Instant::now();
    for (index, item) in fs::read_dir(path)
        .map_err(|_| "permissionDenied")?
        .enumerate()
    {
        if index >= SCAN || started.elapsed() > Duration::from_secs(2) {
            result.truncated = true;
            break;
        }
        let Ok(item) = item else {
            result.truncated = true;
            continue;
        };
        if item.file_name() == ".git" {
            continue;
        }
        let Some(path) = relative(root, &item.path()) else {
            result.truncated = true;
            continue;
        };
        if resolve(root, &path, false).is_err() {
            continue;
        }
        result.entries.push(entry(path, item.path().is_dir()));
    }
    result
        .entries
        .sort_by(|a, b| b.directory.cmp(&a.directory).then(a.path.cmp(&b.path)));
    page(result, params.cursor);
    Ok(())
}
fn page(result: &mut ProjectFilesResult, cursor: u32) {
    let offset = cursor as usize;
    let count = result.entries.len();
    result.entries = result
        .entries
        .drain(offset.min(count)..count.min(offset.saturating_add(PAGE)))
        .collect();
    if count > offset.saturating_add(PAGE) {
        result.next_cursor = Some((offset + PAGE) as u32);
    }
}
fn search(
    root: &Path,
    params: &ProjectFilesParams,
    result: &mut ProjectFilesResult,
) -> Result<(), &'static str> {
    if params.query.trim().is_empty() {
        return Ok(());
    }
    if params.query.len() > 1000 || params.cursor as usize > SCAN {
        return Err("tooLarge");
    }
    let query = if params.case_sensitive {
        params.query.clone()
    } else {
        params.query.to_lowercase()
    };
    let matches = |text: &str| {
        if params.case_sensitive {
            text.contains(&query)
        } else {
            text.to_lowercase().contains(&query)
        }
    };
    let started = Instant::now();
    let mut bytes = 0u64;
    let mut found = 0usize;
    let walker = ignore::WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(!params.include_ignored)
        .git_exclude(!params.include_ignored)
        .git_global(!params.include_ignored)
        .ignore(!params.include_ignored)
        .follow_links(false)
        .sort_by_file_name(|a, b| a.cmp(b))
        .filter_entry(|e| e.file_name() != ".git")
        .build();
    'scan: for (index, item) in walker.enumerate() {
        if index >= SCAN || bytes >= 64 * FILE_BYTES || started.elapsed() > Duration::from_secs(2) {
            result.truncated = true;
            break;
        }
        let Ok(item) = item else {
            result.truncated = true;
            continue;
        };
        if !item.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let Some(path) = relative(root, item.path()) else {
            result.truncated = true;
            continue;
        };
        if resolve(root, &path, false).is_err() {
            continue;
        }
        if !params.content {
            if !matches(&path) {
                continue;
            }
            if found >= params.cursor as usize {
                result.entries.push(entry(path, false));
            }
            found += 1;
        } else {
            let Ok(file) = fs::File::open(item.path()) else {
                result.truncated = true;
                continue;
            };
            let mut content = vec![];
            if file.take(FILE_BYTES + 1).read_to_end(&mut content).is_err() {
                result.truncated = true;
                continue;
            }
            bytes += content.len() as u64;
            if content.len() as u64 > FILE_BYTES {
                result.truncated = true;
                continue;
            }
            let Ok(content) = String::from_utf8(content) else {
                continue;
            };
            if content.contains('\0') {
                continue;
            }
            for (line, text) in content.lines().enumerate() {
                if !matches(text) {
                    continue;
                }
                if found >= params.cursor as usize {
                    let mut e = entry(path.clone(), false);
                    e.line = Some((line + 1) as u32);
                    e.text = Some(text.chars().take(500).collect());
                    result.entries.push(e);
                }
                found += 1;
                if result.entries.len() > PAGE {
                    break 'scan;
                }
            }
        }
        if result.entries.len() > PAGE {
            break;
        }
    }
    if result.entries.len() > PAGE {
        result.entries.truncate(PAGE);
        result.next_cursor = Some(params.cursor + PAGE as u32);
    }
    Ok(())
}
