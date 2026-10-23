//! Host aliases from the user's `~/.ssh/config`, offered when adding an SSH
//! Computer. Only names are read; `ssh` itself applies the configuration.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Bounds `Include` chains, which may refer to each other.
const MAX_INCLUDE_DEPTH: usize = 8;

pub(crate) fn host_aliases(ssh_directory: &Path) -> Vec<String> {
    let mut aliases = BTreeSet::new();
    collect(
        &ssh_directory.join("config"),
        ssh_directory,
        0,
        &mut aliases,
    );
    aliases.into_iter().collect()
}

fn collect(path: &Path, ssh_directory: &Path, depth: usize, aliases: &mut BTreeSet<String>) {
    if depth > MAX_INCLUDE_DEPTH {
        return;
    }
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    for line in text.lines() {
        let Some((keyword, values)) = split_directive(line) else {
            continue;
        };
        if keyword.eq_ignore_ascii_case("host") {
            aliases.extend(
                values
                    .split_whitespace()
                    .map(|alias| alias.trim_matches('"'))
                    .filter(|alias| is_concrete(alias))
                    .map(str::to_string),
            );
        } else if keyword.eq_ignore_ascii_case("include") {
            for pattern in values.split_whitespace() {
                for included in expand_include(pattern.trim_matches('"'), ssh_directory) {
                    collect(&included, ssh_directory, depth + 1, aliases);
                }
            }
        }
    }
}

/// `Keyword value…` or `Keyword=value…`, without comments.
fn split_directive(line: &str) -> Option<(&str, &str)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let split = line.find(|character: char| character.is_whitespace() || character == '=')?;
    let (keyword, rest) = line.split_at(split);
    let rest =
        rest.trim_start_matches(|character: char| character.is_whitespace() || character == '=');
    Some((keyword, rest.split('#').next().unwrap_or_default().trim()))
}

/// Patterns and negations match many hosts and name none.
fn is_concrete(alias: &str) -> bool {
    !alias.is_empty() && !alias.contains(['*', '?', '!', '%'])
}

/// A relative `Include` is under `~/.ssh`. Only the file name may hold `*`,
/// which covers the common `config.d/*` layouts.
fn expand_include(pattern: &str, ssh_directory: &Path) -> Vec<PathBuf> {
    let path = if let Some(rest) = pattern.strip_prefix("~/") {
        match ssh_directory.parent() {
            Some(home) => home.join(rest),
            None => return Vec::new(),
        }
    } else if Path::new(pattern).is_absolute() {
        PathBuf::from(pattern)
    } else {
        ssh_directory.join(pattern)
    };
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    if !name.contains('*') {
        return vec![path];
    }
    let (prefix, suffix) = name.split_once('*').unwrap_or((&name, ""));
    let Some(directory) = path.parent() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut matches: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|candidate| {
            candidate.is_file()
                && candidate.file_name().is_some_and(|file| {
                    let file = file.to_string_lossy();
                    file.len() >= prefix.len() + suffix.len()
                        && file.starts_with(prefix)
                        && file.ends_with(suffix)
                })
        })
        .collect();
    matches.sort();
    matches
}

#[cfg(test)]
#[path = "desktop_ssh_config_tests.rs"]
mod tests;
