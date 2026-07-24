//! Conservative semantic presentation for execute commands.
//!
//! Unknown or mixed shell syntax deliberately returns `None`: these hints only
//! change compact UI chrome, and a false semantic claim is worse than fallback.

use std::path::{Component, Path};
use std::sync::OnceLock;

use regex::Regex;
use serde_json::Value;

use crate::agent::tool_details_sanitizer::{path_leaf_summary, sanitize_command_summary};
use crate::protocol::model::{ToolPresentation, ToolPresentationKind};

mod shell;

use shell::{parse_commands, parse_saved_command};

const MAX_COMMANDS: usize = 8;
const MAX_SUBJECT_BYTES: usize = 512;

pub(crate) fn infer_execute_presentation(raw_input: Option<&Value>) -> Option<ToolPresentation> {
    infer_parsed_commands(parse_commands(raw_input?)?)
}

/// Reconstructs presentation for legacy persisted execute details.
///
/// Saved details contain one sanitized display command rather than ACP raw
/// input, so ambiguous shapes deliberately retain the normal execute fallback.
pub(crate) fn infer_saved_execute_presentation(command: &[String]) -> Option<ToolPresentation> {
    infer_parsed_commands(parse_saved_command(command)?)
}

fn infer_parsed_commands(commands: Vec<Vec<String>>) -> Option<ToolPresentation> {
    if commands.is_empty() || commands.len() > MAX_COMMANDS {
        return None;
    }

    let mut action = None;
    for command in commands {
        let next = classify_command(&command)?;
        match &mut action {
            None => action = Some(next),
            Some(current) if current.kind == next.kind => {
                current.subjects.extend(next.subjects);
            }
            Some(_) => return None,
        }
    }

    let mut action = action?;
    if action.kind == ToolPresentationKind::Read {
        if let Some(subjects) = action
            .subjects
            .iter()
            .map(|path| skill_name(path))
            .collect::<Option<Vec<_>>>()
        {
            action.kind = ToolPresentationKind::Skill;
            action.subjects = subjects;
        }
    }
    presentation(action.kind, ordered_unique(action.subjects))
}

struct SemanticAction {
    kind: ToolPresentationKind,
    subjects: Vec<String>,
}

fn classify_command(words: &[String]) -> Option<SemanticAction> {
    classify_read(words)
        .or_else(|| classify_list(words))
        .or_else(|| classify_search(words))
}

fn classify_read(words: &[String]) -> Option<SemanticAction> {
    if let [program, path] = words {
        if matches!(
            command_name(program)?,
            "cat" | "bat" | "batcat" | "less" | "more" | "nl"
        ) {
            return action(ToolPresentationKind::Read, vec![safe_subject(path)?]);
        }
    }
    if let [program, print_flag, script, path] = words {
        if command_name(program)? == "sed"
            && print_flag == "-n"
            && sed_print_script().is_match(script)
        {
            return action(ToolPresentationKind::Read, vec![safe_subject(path)?]);
        }
    }
    classify_head_or_tail(words)
}

fn classify_head_or_tail(words: &[String]) -> Option<SemanticAction> {
    let program = command_name(words.first()?)?;
    if !matches!(program, "head" | "tail") {
        return None;
    }
    let path = match words {
        [_, path] => path,
        [_, flag, count, path] if flag == "-n" && is_unsigned(count) => path,
        [_, flag, path]
            if flag
                .strip_prefix("-n")
                .or_else(|| flag.strip_prefix("--lines="))
                .or_else(|| flag.strip_prefix('-'))
                .is_some_and(is_unsigned) =>
        {
            path
        }
        _ => return None,
    };
    action(ToolPresentationKind::Read, vec![safe_subject(path)?])
}

fn classify_list(words: &[String]) -> Option<SemanticAction> {
    match command_name(words.first()?)? {
        "ls" | "eza" | "exa" => classify_ls(words),
        "tree" => classify_tree(words),
        "du" => classify_du(words),
        "rg" | "rga" | "ripgrep-all" if words.get(1).is_some_and(|word| word == "--files") => {
            classify_rg_files(words)
        }
        "git" if words.get(1).is_some_and(|word| word == "ls-files") => {
            classify_git_ls_files(words)
        }
        "fd" if words.len() == 1 => action(ToolPresentationKind::List, vec![".".to_string()]),
        _ => None,
    }
}

fn classify_ls(words: &[String]) -> Option<SemanticAction> {
    let mut subjects = Vec::new();
    for word in &words[1..] {
        if word == "--" {
            return None;
        }
        if let Some(flags) = word.strip_prefix('-') {
            if flags.is_empty()
                || !flags
                    .chars()
                    .all(|flag| "1AaBbCcdFfGghHiklLmnopqRrSstUuvXx".contains(flag))
            {
                return None;
            }
        } else {
            subjects.push(safe_subject(word)?);
        }
    }
    if subjects.is_empty() {
        subjects.push(".".to_string());
    }
    action(ToolPresentationKind::List, subjects)
}

fn classify_tree(words: &[String]) -> Option<SemanticAction> {
    let mut subjects = Vec::new();
    for word in &words[1..] {
        if word.starts_with('-') {
            let flags = word.strip_prefix('-')?;
            if flags.is_empty() || !flags.chars().all(|flag| "adfFhins".contains(flag)) {
                return None;
            }
        } else {
            subjects.push(safe_subject(word)?);
        }
    }
    if subjects.is_empty() {
        subjects.push(".".to_string());
    }
    action(ToolPresentationKind::List, subjects)
}

fn classify_du(words: &[String]) -> Option<SemanticAction> {
    let mut subjects = Vec::new();
    for word in &words[1..] {
        if word.starts_with('-') {
            let flags = word.strip_prefix('-')?;
            if flags.is_empty() || !flags.chars().all(|flag| "ahsk".contains(flag)) {
                return None;
            }
        } else {
            subjects.push(safe_subject(word)?);
        }
    }
    if subjects.is_empty() {
        subjects.push(".".to_string());
    }
    action(ToolPresentationKind::List, subjects)
}

fn classify_rg_files(words: &[String]) -> Option<SemanticAction> {
    let mut subjects = Vec::new();
    let mut index = 2;
    while index < words.len() {
        let word = &words[index];
        if matches!(
            word.as_str(),
            "-g" | "--glob" | "-t" | "--type" | "-T" | "--type-not"
        ) {
            index += 2;
            if index > words.len() {
                return None;
            }
            continue;
        }
        if matches!(
            word.as_str(),
            "--hidden" | "--no-ignore" | "--follow" | "--one-file-system"
        ) {
            index += 1;
            continue;
        }
        if word.starts_with('-') {
            return None;
        }
        subjects.push(safe_subject(word)?);
        index += 1;
    }
    if subjects.is_empty() {
        subjects.push(".".to_string());
    }
    action(ToolPresentationKind::List, subjects)
}

fn classify_git_ls_files(words: &[String]) -> Option<SemanticAction> {
    let mut subjects = Vec::new();
    let mut paths = false;
    for word in &words[2..] {
        if word == "--" {
            if paths {
                return None;
            }
            paths = true;
            continue;
        }
        if !paths && word.starts_with('-') {
            if !matches!(
                word.as_str(),
                "-c" | "-d"
                    | "-m"
                    | "-o"
                    | "--cached"
                    | "--deleted"
                    | "--modified"
                    | "--others"
                    | "--ignored"
                    | "--recurse-submodules"
            ) {
                return None;
            }
            continue;
        }
        subjects.push(safe_subject(word)?);
    }
    if subjects.is_empty() {
        subjects.push(".".to_string());
    }
    action(ToolPresentationKind::List, subjects)
}

fn classify_search(words: &[String]) -> Option<SemanticAction> {
    match command_name(words.first()?)? {
        "rg" | "rga" | "ripgrep-all" | "ag" | "ack" | "pt" => {
            classify_search_program(words, SearchOptions::Ripgrep)
        }
        "grep" | "egrep" | "fgrep" => classify_search_program(words, SearchOptions::Grep),
        "git" if words.get(1).is_some_and(|word| word == "grep") => classify_git_grep(words),
        "fd" => classify_fd_search(words),
        "find" => classify_find_search(words),
        _ => None,
    }
}

#[derive(Clone, Copy)]
enum SearchOptions {
    Ripgrep,
    Grep,
}

fn classify_search_program(words: &[String], options: SearchOptions) -> Option<SemanticAction> {
    let mut positionals = Vec::new();
    let mut index = 1;
    let mut options_ended = false;
    while index < words.len() {
        let word = &words[index];
        if !options_ended && word == "--" {
            options_ended = true;
            index += 1;
            continue;
        }
        if !options_ended && search_flag_without_value(word, options) {
            index += 1;
            continue;
        }
        if !options_ended && search_flag_with_value(word, options) {
            index += 2;
            if index > words.len() {
                return None;
            }
            continue;
        }
        if !options_ended && word.starts_with('-') {
            return None;
        }
        positionals.push(word.as_str());
        index += 1;
    }
    let (query, scopes) = positionals.split_first()?;
    search_action(query, scopes)
}

fn search_flag_without_value(word: &str, options: SearchOptions) -> bool {
    match options {
        SearchOptions::Ripgrep => matches!(
            word,
            "-n" | "-i"
                | "-F"
                | "-w"
                | "-x"
                | "-l"
                | "-L"
                | "-c"
                | "-s"
                | "-S"
                | "--hidden"
                | "--no-ignore"
                | "--text"
                | "--follow"
        ),
        SearchOptions::Grep => matches!(
            word,
            "-n" | "-i"
                | "-F"
                | "-E"
                | "-G"
                | "-P"
                | "-w"
                | "-x"
                | "-l"
                | "-L"
                | "-c"
                | "-s"
                | "-R"
                | "-r"
                | "--recursive"
                | "--line-number"
                | "--ignore-case"
        ),
    }
}

fn search_flag_with_value(word: &str, options: SearchOptions) -> bool {
    match options {
        SearchOptions::Ripgrep => matches!(
            word,
            "-g" | "--glob"
                | "-t"
                | "--type"
                | "-T"
                | "--type-not"
                | "-A"
                | "-B"
                | "-C"
                | "--context"
        ),
        SearchOptions::Grep => matches!(word, "-A" | "-B" | "-C" | "--include" | "--exclude"),
    }
}

fn classify_git_grep(words: &[String]) -> Option<SemanticAction> {
    let mut positionals = Vec::new();
    let mut options_ended = false;
    for word in &words[2..] {
        if !options_ended && word == "--" {
            options_ended = true;
        } else if !options_ended
            && matches!(
                word.as_str(),
                "-n" | "-i"
                    | "-F"
                    | "-E"
                    | "-G"
                    | "-P"
                    | "-w"
                    | "-l"
                    | "--cached"
                    | "--untracked"
                    | "--no-index"
            )
        {
            continue;
        } else if !options_ended && word.starts_with('-') {
            return None;
        } else {
            positionals.push(word.as_str());
        }
    }
    let (query, scopes) = positionals.split_first()?;
    search_action(query, scopes)
}

fn classify_fd_search(words: &[String]) -> Option<SemanticAction> {
    match words {
        [_, query] => search_action(query, &[]),
        [_, query, path] => search_action(query, &[path.as_str()]),
        _ => None,
    }
}

fn classify_find_search(words: &[String]) -> Option<SemanticAction> {
    let [_, path, name_flag, query, rest @ ..] = words else {
        return None;
    };
    if !matches!(name_flag.as_str(), "-name" | "-iname") {
        return None;
    }
    if !rest.is_empty() && rest != ["-print"] {
        return None;
    }
    search_action(query, &[path.as_str()])
}

fn search_action(query: &str, scopes: &[&str]) -> Option<SemanticAction> {
    let query = safe_subject(query)?;
    let subject = if scopes.is_empty() {
        query
    } else {
        let scopes = scopes
            .iter()
            .map(|scope| safe_subject(scope))
            .collect::<Option<Vec<_>>>()?;
        format!("{query} in {}", scopes.join(", "))
    };
    action(ToolPresentationKind::Search, vec![subject])
}

fn action(kind: ToolPresentationKind, subjects: Vec<String>) -> Option<SemanticAction> {
    (!subjects.is_empty()).then_some(SemanticAction { kind, subjects })
}

fn is_unsigned(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn command_name(program: &str) -> Option<&str> {
    let path = Path::new(program);
    let name = path.file_name()?.to_str()?;
    if path
        .parent()
        .is_none_or(|parent| parent.as_os_str().is_empty())
        || matches!(
            path.parent().and_then(Path::to_str),
            Some("/bin" | "/usr/bin")
        )
    {
        Some(name)
    } else {
        None
    }
}

fn sed_print_script() -> &'static Regex {
    static SCRIPT: OnceLock<Regex> = OnceLock::new();
    SCRIPT.get_or_init(|| Regex::new(r"^\d+(?:,\d+)?p$").expect("valid sed print regex"))
}

fn safe_subject(value: &str) -> Option<String> {
    let subject = value.trim();
    if subject.is_empty()
        || subject == "-"
        || subject.len() > MAX_SUBJECT_BYTES
        || subject.starts_with('-')
        || subject.chars().any(char::is_control)
    {
        return None;
    }
    Some(subject.to_string())
}

fn skill_name(path: &str) -> Option<String> {
    let path = Path::new(path);
    if path
        .components()
        .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return None;
    }
    if path.file_name()?.to_str()? != "SKILL.md" {
        return None;
    }
    let directory = path.parent()?;
    if !directory
        .ancestors()
        .skip(1)
        .any(|ancestor| ancestor.file_name().and_then(|name| name.to_str()) == Some("skills"))
    {
        return None;
    }
    safe_subject(directory.file_name()?.to_str()?)
}

fn presentation(kind: ToolPresentationKind, subjects: Vec<String>) -> Option<ToolPresentation> {
    let subjects = ordered_unique(
        subjects
            .into_iter()
            .map(|subject| match kind {
                ToolPresentationKind::Read => path_leaf_summary(&subject),
                _ => sanitize_command_summary(&subject),
            })
            .filter(|subject| !subject.is_empty())
            .collect(),
    );
    (!subjects.is_empty() && subjects.len() <= MAX_COMMANDS)
        .then_some(ToolPresentation { kind, subjects })
}

fn ordered_unique(subjects: Vec<String>) -> Vec<String> {
    let mut unique = Vec::with_capacity(subjects.len());
    for subject in subjects {
        if !unique.contains(&subject) {
            unique.push(subject);
        }
    }
    unique
}
