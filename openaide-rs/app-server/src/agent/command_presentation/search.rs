use crate::protocol::model::ToolSearchTarget;

use super::{command_name, safe_subject, SemanticAction};

pub(super) fn classify_search(words: &[String]) -> Option<SemanticAction> {
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
pub(super) enum SearchOptions {
    Ripgrep,
    Grep,
}

pub(super) fn classify_search_program(
    words: &[String],
    options: SearchOptions,
) -> Option<SemanticAction> {
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
    Some(SemanticAction::Search {
        query: safe_subject(query)?,
        scopes: scopes
            .iter()
            .map(|scope| safe_subject(scope))
            .collect::<Option<Vec<_>>>()?,
        target: ToolSearchTarget::Contents,
    })
}
