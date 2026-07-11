use crate::time::now_string;

pub(super) const TRACE_ENV: &str = "OPENAIDE_ACP_TRACE";
pub(super) const TRACE_DIR_ENV: &str = "OPENAIDE_ACP_TRACE_DIR";

pub(super) fn trace_enabled(value: Option<&str>) -> bool {
    matches!(
        value.map(str::trim).map(str::to_ascii_lowercase).as_deref(),
        Some("1" | "true" | "yes" | "on" | "full" | "raw")
    )
}

pub(super) fn compact_timestamp() -> String {
    now_string()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

pub(super) fn safe_file_segment(value: &str) -> String {
    let mut segment = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    segment.truncate(80);
    if segment.is_empty() {
        "unknown".to_string()
    } else {
        segment
    }
}
