use std::path::Path;

use serde_json::Value;

pub(super) fn parse_commands(raw_input: &Value) -> Option<Vec<Vec<String>>> {
    match command_input(raw_input)? {
        CommandInput::Source(source) => {
            let words = shell_words::split(&source).ok()?;
            let source = shell_body(&words).unwrap_or(&source);
            parse_source(source)
        }
        CommandInput::Argv(words) => {
            if let Some(source) = shell_body(&words) {
                parse_source(source)
            } else {
                Some(vec![words])
            }
        }
    }
}

pub(super) fn parse_saved_command(command: &[String]) -> Option<Vec<Vec<String>>> {
    let [source] = command else {
        return None;
    };
    let words = shell_words::split(source).ok()?;
    let source = shell_body(&words).unwrap_or(source);
    parse_source(source)
}

enum CommandInput {
    Source(String),
    Argv(Vec<String>),
}

fn command_input(raw_input: &Value) -> Option<CommandInput> {
    let input = raw_input.as_object()?;
    let command = input
        .get("command")
        .filter(|value| matches!(value, Value::String(_) | Value::Array(_)));
    let cmd = input
        .get("cmd")
        .filter(|value| matches!(value, Value::String(_) | Value::Array(_)));
    let value = match (command, cmd) {
        (Some(command), Some(cmd)) if command != cmd => return None,
        (Some(command), _) => command,
        (_, Some(cmd)) => cmd,
        (None, None) => return None,
    };
    match value {
        Value::String(source) => Some(CommandInput::Source(source.clone())),
        Value::Array(values) => values
            .iter()
            .map(Value::as_str)
            .map(|value| value.map(str::to_string))
            .collect::<Option<Vec<String>>>()
            .map(CommandInput::Argv),
        _ => None,
    }
}

fn parse_source(source: &str) -> Option<Vec<Vec<String>>> {
    split_plain_commands(source)?
        .into_iter()
        .map(|command| shell_words::split(command).ok())
        .collect()
}

fn shell_body<S: AsRef<str>>(words: &[S]) -> Option<&str> {
    if words.len() != 3 {
        return None;
    }
    let path = Path::new(words[0].as_ref());
    if !path
        .parent()
        .is_none_or(|parent| parent.as_os_str().is_empty())
        && !matches!(
            path.parent().and_then(Path::to_str),
            Some("/bin" | "/usr/bin")
        )
    {
        return None;
    }
    let shell = path.file_name()?.to_str()?;
    if !matches!(shell, "sh" | "bash" | "zsh") || !matches!(words[1].as_ref(), "-c" | "-lc") {
        return None;
    }
    Some(words[2].as_ref())
}

/// Splits only plain `&&` or `;` command lists. Classification later requires
/// every segment to have the same proven read/list/search meaning.
///
/// Other operators, expansions, substitutions, and redirections deliberately
/// fall back to execute.
fn split_plain_commands(source: &str) -> Option<Vec<&str>> {
    let bytes = source.as_bytes();
    let mut quote = None;
    let mut escaped = false;
    let mut start = 0;
    let mut commands = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if escaped {
            escaped = false;
            index += 1;
            continue;
        }
        if byte == b'\\' && quote != Some(b'\'') {
            escaped = true;
            index += 1;
            continue;
        }
        if let Some(active) = quote {
            if byte == active {
                quote = None;
            } else if active == b'"' && matches!(byte, b'$' | b'`') {
                return None;
            }
            index += 1;
            continue;
        }
        if matches!(byte, b'\'' | b'"') {
            quote = Some(byte);
            index += 1;
            continue;
        }
        if byte == b'&' && bytes.get(index + 1) == Some(&b'&') {
            let command = source[start..index].trim();
            if command.is_empty() {
                return None;
            }
            commands.push(command);
            index += 2;
            start = index;
            continue;
        }
        if byte == b';' {
            let command = source[start..index].trim();
            if command.is_empty() {
                return None;
            }
            commands.push(command);
            index += 1;
            start = index;
            continue;
        }
        if matches!(
            byte,
            b'|' | b'&'
                | b'<'
                | b'>'
                | b'`'
                | b'$'
                | b'('
                | b')'
                | b'{'
                | b'}'
                | b'*'
                | b'?'
                | b'['
                | b']'
                | b'!'
                | b'\n'
        ) {
            return None;
        }
        index += 1;
    }
    if quote.is_some() || escaped {
        return None;
    }
    let command = source[start..].trim();
    if command.is_empty() {
        return None;
    }
    commands.push(command);
    Some(commands)
}
