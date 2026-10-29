use super::*;
use std::{
    process::{Command, Stdio},
    thread,
};

/// Disable Git's optional external helpers. Both output and runtime are bounded;
/// stdout is drained concurrently so a full pipe cannot deadlock the deadline check.
fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>, &'static str> {
    let mut child = Command::new("git")
        .arg("--no-pager")
        .arg("--literal-pathspecs")
        .args([
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.untrackedCache=false",
            "-c",
            "diff.external=",
            "-c",
            "core.quotePath=false",
            "-C",
        ])
        .arg(root)
        .args(args)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env_remove("GIT_EXTERNAL_DIFF")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "unavailable")?;
    let stdout = child.stdout.take().ok_or("unavailable")?;
    let reader = thread::spawn(move || {
        let mut bytes = vec![];
        stdout
            .take(2 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => {}
            Err(_) => break Err("unavailable"),
        }
        if started.elapsed() > Duration::from_secs(5) {
            break Err("timeout");
        }
        thread::sleep(Duration::from_millis(10));
    };
    if status.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    let bytes = reader
        .join()
        .map_err(|_| "unavailable")?
        .map_err(|_| "unavailable")?;
    let status = status?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err("tooLarge");
    }
    if !status.success() {
        return Err("unavailable");
    }
    Ok(bytes)
}
fn text(root: &Path, args: &[&str]) -> Option<String> {
    git(root, args)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
/// The default branch is the remote's HEAD when a remote-tracking ref records it, then the
/// conventional remote and local names. Nothing is fetched, so a stale remote ref is used as is.
fn default_branch(root: &Path) -> Option<(String, String)> {
    if let Some(remote) = text(
        root,
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
    ) {
        let name = remote
            .strip_prefix("origin/")
            .unwrap_or(&remote)
            .to_string();
        return Some((remote, name));
    }
    for name in ["main", "master"] {
        for reference in [
            format!("refs/remotes/origin/{name}"),
            format!("refs/heads/{name}"),
        ] {
            if git(root, &["rev-parse", "--verify", "--quiet", &reference]).is_ok() {
                return Some((reference, name.to_string()));
            }
        }
    }
    None
}
/// Resolves the repository, HEAD, and the commit that changes are compared against.
/// `result.base` is that commit; `result.default_branch` is offered only when comparing
/// against it would show something other than uncommitted work.
fn context(
    root: &Path,
    params: &ProjectFilesParams,
    result: &mut ProjectFilesResult,
) -> Result<(), &'static str> {
    let top = git(root, &["rev-parse", "--show-toplevel"]).map_err(|_| "notRepository")?;
    let top = PathBuf::from(
        String::from_utf8(top)
            .map_err(|_| "unavailable")?
            .trim_end(),
    );
    // Workspaces are repository/worktree roots, never inferred parent repositories.
    if fs::canonicalize(top).map_err(|_| "notRepository")? != root {
        return Err("notRepository");
    }
    result.branch = text(root, &["symbolic-ref", "--short", "HEAD"]);
    let head = text(root, &["rev-parse", "--verify", "HEAD"]);
    let merge_base = head.as_ref().and_then(|_| {
        let (reference, name) = default_branch(root)?;
        let commit = text(root, &["merge-base", "HEAD", &reference])?;
        Some((commit, name))
    });
    if let Some((commit, name)) = &merge_base {
        if Some(commit) != head.as_ref() {
            result.default_branch = Some(name.clone());
        }
    }
    result.base = match params.base {
        Some(ProjectChangeBase::DefaultBranch) => Some(
            merge_base
                .map(|(commit, _)| commit)
                .ok_or("noDefaultBranch")?,
        ),
        _ => head,
    };
    Ok(())
}
fn status_name(code: &str) -> &'static str {
    if code.contains('U') || code == "AA" || code == "DD" {
        "conflicted"
    } else if code == "??" {
        "untracked"
    } else if code.contains('R') {
        "renamed"
    } else if code.contains('D') {
        "deleted"
    } else if code.contains('A') {
        "added"
    } else {
        "modified"
    }
}
fn status(root: &Path) -> Result<Vec<ProjectFileEntry>, &'static str> {
    let bytes = git(
        root,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?;
    let mut records = bytes.split(|b| *b == 0).filter(|r| !r.is_empty());
    let mut entries = vec![];
    while let Some(record) = records.next() {
        if record.len() < 4 {
            return Err("unavailable");
        }
        let code = std::str::from_utf8(&record[..2]).map_err(|_| "unavailable")?;
        let path = std::str::from_utf8(&record[3..])
            .map_err(|_| "unavailable")?
            .to_string();
        let old = if code.contains('R') || code.contains('C') {
            Some(
                std::str::from_utf8(records.next().ok_or("unavailable")?)
                    .map_err(|_| "unavailable")?
                    .to_string(),
            )
        } else {
            None
        };
        if resolve(root, &path, true).is_err() {
            continue;
        }
        let mut item = entry(path, false);
        item.old_path = old;
        item.status = Some(status_name(code).into());
        entries.push(item);
    }
    Ok(entries)
}
/// Files that differ between `base` and the working tree, committed or not. Untracked and
/// conflicted files are not part of that diff and come from the status listing.
fn since(root: &Path, base: &str) -> Result<Vec<ProjectFileEntry>, &'static str> {
    let bytes = git(
        root,
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--name-status",
            "-z",
            "-M",
            base,
        ],
    )?;
    let mut records = bytes.split(|b| *b == 0).filter(|r| !r.is_empty());
    let mut entries = vec![];
    while let Some(code) = records.next() {
        let code = std::str::from_utf8(code).map_err(|_| "unavailable")?;
        let mut name = || -> Result<String, &'static str> {
            Ok(std::str::from_utf8(records.next().ok_or("unavailable")?)
                .map_err(|_| "unavailable")?
                .to_string())
        };
        let (old, path) = if code.starts_with('R') || code.starts_with('C') {
            (Some(name()?), name()?)
        } else {
            (None, name()?)
        };
        if resolve(root, &path, true).is_err() {
            continue;
        }
        let mut item = entry(path, false);
        item.old_path = old.filter(|_| code.starts_with('R'));
        item.status = Some(status_name(&code[..1]).into());
        entries.push(item);
    }
    let working = status(root)?;
    for item in working {
        match item.status.as_deref() {
            Some("conflicted") => {
                entries.retain(|entry| entry.path != item.path);
                entries.push(item);
            }
            Some("untracked") => entries.push(item),
            _ => {}
        }
    }
    Ok(entries)
}
/// Added and removed line counts per path from one `--numstat` pass. Binary files report
/// no counts. Renames carry their new path after an empty first field.
fn numstat(root: &Path, base: &str) -> Result<Vec<(String, u32, u32)>, &'static str> {
    let bytes = git(
        root,
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--numstat",
            "-z",
            "-M",
            base,
        ],
    )?;
    let mut records = bytes.split(|b| *b == 0);
    let mut counts = vec![];
    while let Some(record) = records.next() {
        if record.is_empty() {
            continue;
        }
        let record = std::str::from_utf8(record).map_err(|_| "unavailable")?;
        let mut fields = record.splitn(3, '\t');
        let added = fields.next().unwrap_or_default();
        let removed = fields.next().unwrap_or_default();
        let mut path = fields.next().ok_or("unavailable")?.to_string();
        if path.is_empty() {
            records.next().ok_or("unavailable")?;
            path = std::str::from_utf8(records.next().ok_or("unavailable")?)
                .map_err(|_| "unavailable")?
                .to_string();
        }
        if let (Ok(added), Ok(removed)) = (added.parse(), removed.parse()) {
            counts.push((path, added, removed));
        }
    }
    Ok(counts)
}
/// A file with no counterpart in the base is all additions; count its lines within the
/// same bound a diff of it would be read with.
fn new_file_lines(path: &Path) -> Option<u32> {
    let file = fs::File::open(path).ok()?;
    let mut bytes = vec![];
    file.take(FILE_BYTES + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > FILE_BYTES || bytes.contains(&0) {
        return None;
    }
    Some(std::str::from_utf8(&bytes).ok()?.lines().count() as u32)
}
fn change_list(
    root: &Path,
    params: &ProjectFilesParams,
    base: Option<&str>,
) -> Result<Vec<ProjectFileEntry>, &'static str> {
    match (params.base, base) {
        (Some(ProjectChangeBase::DefaultBranch), Some(base)) => since(root, base),
        _ => status(root),
    }
}
pub(super) fn changes(
    root: &Path,
    params: &ProjectFilesParams,
    result: &mut ProjectFilesResult,
) -> Result<(), &'static str> {
    context(root, params, result)?;
    result.entries = change_list(root, params, result.base.as_deref())?;
    result.entries.sort_by(|a, b| a.path.cmp(&b.path));
    result.total = Some(result.entries.len() as u32);
    page(result, params.cursor);
    // Counts are best effort: a failed or oversized pass leaves the list usable without them.
    let counts = match result.base.as_deref() {
        Some(base) => numstat(root, base).unwrap_or_default(),
        None => vec![],
    };
    for item in &mut result.entries {
        if item.status.as_deref() == Some("conflicted") {
            continue;
        }
        if let Some((_, added, removed)) = counts.iter().find(|(path, _, _)| *path == item.path) {
            item.added = Some(*added);
            item.removed = Some(*removed);
        } else if matches!(item.status.as_deref(), Some("untracked" | "added")) {
            item.added = new_file_lines(&root.join(&item.path));
            item.removed = item.added.map(|_| 0);
        }
    }
    Ok(())
}
pub(super) fn diff(
    root: &Path,
    params: &ProjectFilesParams,
    result: &mut ProjectFilesResult,
) -> Result<(), &'static str> {
    context(root, params, result)?;
    let path = resolve(root, &params.path, true)?;
    let item = change_list(root, params, result.base.as_deref())?
        .into_iter()
        .find(|e| e.path == params.path)
        .ok_or("changed")?;
    let mut diff = ProjectFileDiff {
        path: params.path.clone(),
        old_path: item.old_path.clone(),
        binary: false,
        conflicted: item.status.as_deref() == Some("conflicted"),
        hunks: vec![],
    };
    if diff.conflicted {
        result.diff = Some(diff);
        return Ok(());
    }
    if item.status.as_deref() == Some("untracked") || result.base.is_none() {
        let file = fs::File::open(path).map_err(|_| "unavailable")?;
        let mut bytes = vec![];
        file.take(FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "unavailable")?;
        if bytes.len() as u64 > FILE_BYTES {
            return Err("tooLarge");
        }
        match String::from_utf8(bytes) {
            Ok(text) if !text.contains('\0') => diff.hunks.push(ProjectDiffHunk {
                heading: "New file".into(),
                lines: text
                    .lines()
                    .enumerate()
                    .map(|(i, text)| ProjectDiffLine {
                        kind: "add".into(),
                        text: text.into(),
                        old_line: None,
                        new_line: Some((i + 1) as u32),
                    })
                    .collect(),
            }),
            _ => diff.binary = true,
        }
    } else {
        let base = result.base.as_deref().ok_or("unavailable")?;
        let mut args = vec![
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--unified=3",
            base,
            "--",
            &params.path,
        ];
        if let Some(old) = &item.old_path {
            resolve(root, old, true)?;
            args.push(old);
        }
        let bytes = git(root, &args)?;
        let output = String::from_utf8(bytes).map_err(|_| "unavailable")?;
        diff.binary = output
            .lines()
            .any(|l| l.starts_with("Binary files ") || l == "GIT binary patch");
        diff.hunks = parse_hunks(&output)?;
    }
    result.diff = Some(diff);
    Ok(())
}
fn parse_hunks(output: &str) -> Result<Vec<ProjectDiffHunk>, &'static str> {
    let mut hunks: Vec<ProjectDiffHunk> = vec![];
    let mut old = 0;
    let mut new = 0;
    let mut in_hunk = false;
    for line in output.lines() {
        if line.starts_with("diff --git ") {
            in_hunk = false;
        }
        if line.starts_with("@@ ") {
            in_hunk = true;
            let mut fields = line.split_whitespace();
            fields.next();
            let number = |part: Option<&str>| {
                part.and_then(|s| s.get(1..))
                    .and_then(|s| s.split(',').next())
                    .and_then(|s| s.parse::<u32>().ok())
                    .ok_or("unavailable")
            };
            old = number(fields.next())?;
            new = number(fields.next())?;
            hunks.push(ProjectDiffHunk {
                heading: line.into(),
                lines: vec![],
            });
            continue;
        }
        if !in_hunk {
            continue;
        }
        let Some(hunk) = hunks.last_mut() else {
            continue;
        };
        let (kind, old_line, new_line) = match line.as_bytes().first() {
            Some(b'+') => {
                let n = new;
                new += 1;
                ("add", None, Some(n))
            }
            Some(b'-') => {
                let n = old;
                old += 1;
                ("remove", Some(n), None)
            }
            Some(b' ') => {
                let a = old;
                let b = new;
                old += 1;
                new += 1;
                ("context", Some(a), Some(b))
            }
            _ => continue,
        };
        hunk.lines.push(ProjectDiffLine {
            kind: kind.into(),
            text: line[1..].into(),
            old_line,
            new_line,
        });
    }
    Ok(hunks)
}
