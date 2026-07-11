use std::path::Path;

use crate::protocol::model::Attachment;

pub(super) fn attachment_resource_name(attachment: &Attachment) -> String {
    let label = attachment.label.trim();
    if !label.is_empty() {
        return label.to_string();
    }
    attachment
        .path
        .as_deref()
        .and_then(|path| Path::new(path).file_name())
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("context")
        .to_string()
}

pub(super) fn attachment_resource_uri(attachment: &Attachment) -> Option<String> {
    let path = attachment.path.as_deref()?.trim();
    if path.is_empty() {
        return None;
    }
    let normalized = path.replace('\\', "/");
    if Path::new(path).is_absolute() || is_windows_absolute_path(&normalized) {
        file_path_uri(&normalized)
    } else if has_uri_scheme(&normalized) {
        file_uri(&normalized)
    } else {
        None
    }
}

pub(super) fn embedded_attachment_uri(attachment: &Attachment) -> String {
    format!(
        "openaide://attachment/{}",
        percent_encode_path(&format!("/{}", attachment_resource_name(attachment)))
            .trim_start_matches('/')
    )
}

fn file_path_uri(path: &str) -> Option<String> {
    let normalized = path.trim().replace('\\', "/");
    if normalized.is_empty() {
        return None;
    }
    let path = if is_windows_absolute_path(&normalized) {
        format!("/{normalized}")
    } else {
        normalized
    };
    if !path.starts_with('/') {
        return None;
    }
    Some(format!("file://{}", percent_encode_path(&path)))
}

fn file_uri(uri: &str) -> Option<String> {
    let normalized = uri.trim().replace('\\', "/");
    let scheme_end = normalized.find(':')?;
    if !normalized[..scheme_end].eq_ignore_ascii_case("file") {
        return None;
    }
    let authority_and_path = normalized[scheme_end + 1..].strip_prefix("//")?;
    let path = if authority_and_path.starts_with('/') {
        authority_and_path
    } else if authority_and_path.len() > "localhost".len()
        && authority_and_path[.."localhost".len()].eq_ignore_ascii_case("localhost")
        && authority_and_path["localhost".len()..].starts_with('/')
    {
        &authority_and_path["localhost".len()..]
    } else {
        return None;
    };
    if !path.starts_with('/') {
        return None;
    }
    Some(format!("file://{}", percent_encode_uri_path(path)))
}

fn has_uri_scheme(value: &str) -> bool {
    let Some(scheme_end) = value.find(':') else {
        return false;
    };
    if scheme_end == 0 {
        return false;
    }
    value[..scheme_end]
        .bytes()
        .enumerate()
        .all(|(index, byte)| {
            if index == 0 {
                byte.is_ascii_alphabetic()
            } else {
                byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.')
            }
        })
}

fn is_windows_absolute_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'/'
}

fn percent_encode_path(path: &str) -> String {
    let mut encoded = String::new();
    for byte in path.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                encoded.push(char::from(*byte))
            }
            value => encoded.push_str(&format!("%{value:02X}")),
        }
    }
    encoded
}

fn percent_encode_uri_path(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut encoded = String::new();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'%'
            && index + 2 < bytes.len()
            && bytes[index + 1].is_ascii_hexdigit()
            && bytes[index + 2].is_ascii_hexdigit()
        {
            encoded.push('%');
            encoded.push(char::from(bytes[index + 1]));
            encoded.push(char::from(bytes[index + 2]));
            index += 3;
            continue;
        }
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                encoded.push(char::from(byte))
            }
            value => encoded.push_str(&format!("%{value:02X}")),
        }
        index += 1;
    }
    encoded
}
