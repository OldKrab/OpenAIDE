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
fn context(root: &Path, result: &mut ProjectFilesResult) -> Result<(), &'static str> {
    let top = git(root, &["rev-parse", "--show-toplevel"]).map_err(|_| "notRepository")?;
    let top = PathBuf::from(
        String::from_utf8(top)
            .map_err(|_| "unavailable")?
            .trim_end(),
    );
    // Task Workspaces are repository/worktree roots, never inferred parent repositories.
    if fs::canonicalize(top).map_err(|_| "notRepository")? != root {
        return Err("notRepository");
    }
    result.branch = git(root, &["symbolic-ref", "--short", "HEAD"])
        .ok()
        .and_then(|b| String::from_utf8(b).ok())
        .map(|s| s.trim().into());
    result.base = git(root, &["rev-parse", "--verify", "HEAD"])
        .ok()
        .and_then(|b| String::from_utf8(b).ok())
        .map(|s| s.trim().into());
    Ok(())
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
        item.status = Some(
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
            .into(),
        );
        entries.push(item);
    }
    Ok(entries)
}
pub(super) fn changes(
    root: &Path,
    params: &ProjectFilesParams,
    result: &mut ProjectFilesResult,
) -> Result<(), &'static str> {
    context(root, result)?;
    result.entries = status(root)?;
    result.entries.sort_by(|a, b| a.path.cmp(&b.path));
    page(result, params.cursor);
    Ok(())
}
pub(super) fn diff(
    root: &Path,
    params: &ProjectFilesParams,
    result: &mut ProjectFilesResult,
) -> Result<(), &'static str> {
    context(root, result)?;
    let path = resolve(root, &params.path, true)?;
    let item = status(root)?
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
