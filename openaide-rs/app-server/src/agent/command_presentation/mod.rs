//! Conservative semantic presentation for execute commands.
//!
//! Unknown or unsupported mixed shell syntax deliberately returns `None`: these
//! hints only change compact UI chrome, and a false semantic claim is worse than
//! fallback.

use std::path::{Component, Path};
use std::sync::OnceLock;

use regex::Regex;
use serde_json::Value;

use crate::agent::tool_details_sanitizer::{path_leaf_summary, sanitize_command_summary};
use crate::protocol::model::{
    ToolPresentation, ToolPresentationAction, ToolPresentationKind, ToolSearchTarget,
};

mod search;
mod shell;

use search::{classify_search, classify_search_program, SearchOptions};
use shell::{parse_commands, parse_saved_command, ParsedCommand};

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

fn infer_parsed_commands(commands: Vec<ParsedCommand>) -> Option<ToolPresentation> {
    if commands.is_empty() || commands.len() > MAX_COMMANDS {
        return None;
    }

    let mut actions: Vec<SemanticAction> = Vec::new();
    for command in commands {
        let next = classify_pipeline(&command.stages)?;
        if let Some(current) = actions.last_mut() {
            match current.merge(next) {
                Ok(()) => continue,
                Err(next) => actions.push(next),
            }
        } else {
            actions.push(next);
        }
    }

    if actions.len() > 1 && !is_supported_action_sequence(&actions) {
        return None;
    }
    for action in &mut actions {
        if let SemanticAction::Subjects {
            kind: ToolPresentationKind::Read,
            subjects,
        } = action
        {
            if let Some(skill_names) = subjects
                .iter()
                .map(|path| skill_name(path))
                .collect::<Option<Vec<_>>>()
            {
                *action = SemanticAction::Subjects {
                    kind: ToolPresentationKind::Skill,
                    subjects: skill_names,
                };
            }
        }
    }
    presentation(actions)
}

/// A pipeline receives semantic chrome only when every stage and their data-flow
/// relationship are on this small allowlist.
fn classify_pipeline(stages: &[Vec<String>]) -> Option<SemanticAction> {
    match stages {
        [command] => classify_command(command),
        [search, limiter] if is_stdin_head_limiter(limiter) => {
            let action = classify_search(search)?;
            matches!(action, SemanticAction::Search { .. }).then_some(action)
        }
        [files, filter] => classify_file_name_search(files, filter),
        _ => None,
    }
}

fn is_stdin_head_limiter(words: &[String]) -> bool {
    if command_name(words.first().map(String::as_str).unwrap_or_default()) != Some("head") {
        return false;
    }
    match words {
        [_] => true,
        [_, flag] => flag
            .strip_prefix("--lines=")
            .or_else(|| flag.strip_prefix('-'))
            .is_some_and(is_unsigned),
        [_, flag, count] => flag == "-n" && is_unsigned(count),
        _ => false,
    }
}

fn classify_file_name_search(files: &[String], filter: &[String]) -> Option<SemanticAction> {
    if command_name(files.first()?)? != "rg" || command_name(filter.first()?)? != "rg" {
        return None;
    }
    let SemanticAction::Subjects {
        kind: ToolPresentationKind::List,
        subjects: scopes,
    } = classify_rg_files(files)?
    else {
        return None;
    };
    let SemanticAction::Search {
        query,
        scopes: filter_scopes,
        ..
    } = classify_search_program(filter, SearchOptions::Ripgrep)?
    else {
        return None;
    };
    if !filter_scopes.is_empty() {
        return None;
    }
    Some(SemanticAction::Search {
        query,
        scopes,
        target: ToolSearchTarget::Paths,
    })
}

enum SemanticAction {
    Subjects {
        kind: ToolPresentationKind,
        subjects: Vec<String>,
    },
    Search {
        query: String,
        scopes: Vec<String>,
        target: ToolSearchTarget,
    },
}

impl SemanticAction {
    fn kind(&self) -> ToolPresentationKind {
        match self {
            Self::Subjects { kind, .. } => *kind,
            Self::Search { .. } => ToolPresentationKind::Search,
        }
    }

    /// Only subject-list actions share one natural-language verb and can merge.
    fn merge(&mut self, next: Self) -> Result<(), Self> {
        match (self, next) {
            (
                Self::Subjects {
                    kind: current_kind,
                    subjects: current_subjects,
                },
                Self::Subjects {
                    kind: next_kind,
                    subjects: next_subjects,
                },
            ) if *current_kind == next_kind => {
                current_subjects.extend(next_subjects);
                Ok(())
            }
            (_, next) => Err(next),
        }
    }
}

/// Mixed inference stays deliberately narrow: only fully classified Read + Search
/// command lists may replace Execute chrome. Every other combination falls back.
fn is_supported_action_sequence(actions: &[SemanticAction]) -> bool {
    if actions
        .iter()
        .all(|action| action.kind() == actions[0].kind())
    {
        return true;
    }
    actions.iter().all(|action| {
        matches!(
            action.kind(),
            ToolPresentationKind::Read | ToolPresentationKind::Search
        )
    }) && actions
        .iter()
        .any(|action| action.kind() == ToolPresentationKind::Read)
        && actions
            .iter()
            .any(|action| action.kind() == ToolPresentationKind::Search)
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

fn action(kind: ToolPresentationKind, subjects: Vec<String>) -> Option<SemanticAction> {
    (!subjects.is_empty()).then_some(SemanticAction::Subjects { kind, subjects })
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

fn presentation(actions: Vec<SemanticAction>) -> Option<ToolPresentation> {
    let mut subject_count = 0;
    let actions = actions
        .into_iter()
        .map(|action| match action {
            SemanticAction::Subjects { kind, subjects } => {
                let subjects = ordered_unique(
                    subjects
                        .into_iter()
                        .map(|subject| match kind {
                            ToolPresentationKind::Read | ToolPresentationKind::View => {
                                path_leaf_summary(&subject)
                            }
                            _ => sanitize_command_summary(&subject),
                        })
                        .filter(|subject| !subject.is_empty())
                        .collect(),
                );
                subject_count += subjects.len();
                let action = match kind {
                    ToolPresentationKind::Skill => ToolPresentationAction::Skill { subjects },
                    ToolPresentationKind::Read => ToolPresentationAction::Read { subjects },
                    ToolPresentationKind::View => ToolPresentationAction::View { subjects },
                    ToolPresentationKind::List => ToolPresentationAction::List { subjects },
                    ToolPresentationKind::Search => return None,
                };
                action
                    .subjects()
                    .is_some_and(|subjects| !subjects.is_empty())
                    .then_some(action)
            }
            SemanticAction::Search {
                query,
                scopes,
                target,
            } => {
                let query = sanitize_command_summary(&query);
                let scopes = ordered_unique(
                    scopes
                        .into_iter()
                        .map(|scope| sanitize_command_summary(&scope))
                        .collect(),
                );
                subject_count += 1;
                (!query.is_empty() && scopes.iter().all(|scope| !scope.is_empty())).then_some(
                    ToolPresentationAction::Search {
                        query,
                        scopes,
                        target,
                    },
                )
            }
        })
        .collect::<Option<Vec<_>>>()?;
    (!actions.is_empty() && subject_count <= MAX_COMMANDS).then_some(ToolPresentation { actions })
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
