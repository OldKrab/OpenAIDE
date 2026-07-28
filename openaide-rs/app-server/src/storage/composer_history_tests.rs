use super::{ComposerHistory, ComposerHistoryEntryRecord};

#[test]
fn exact_duplicate_moves_to_newest_without_changing_identity() {
    let mut history = ComposerHistory::default();
    history.record(entry("first-id", "first", "1"));
    history.record(entry("second-id", "second", "2"));
    history.record(entry("replacement-id", "first", "3"));

    assert_eq!(
        history
            .entries()
            .iter()
            .map(|entry| (
                entry.entry_id.as_str(),
                entry.text.as_str(),
                entry.accepted_at.as_str()
            ))
            .collect::<Vec<_>>(),
        vec![("first-id", "first", "3"), ("second-id", "second", "2")]
    );
}

#[test]
fn task_projection_keeps_only_fifty_newest_values() {
    let mut history = ComposerHistory::default();
    for index in 0..51 {
        history.record(entry(
            &format!("entry-{index}"),
            &format!("message-{index}"),
            &index.to_string(),
        ));
    }

    assert_eq!(history.entries().len(), 50);
    assert_eq!(history.entries()[0].text, "message-50");
    assert_eq!(history.entries()[49].text, "message-1");
}

fn entry(entry_id: &str, text: &str, accepted_at: &str) -> ComposerHistoryEntryRecord {
    ComposerHistoryEntryRecord {
        entry_id: entry_id.to_string(),
        project_id: "project".to_string(),
        text: text.to_string(),
        accepted_at: accepted_at.to_string(),
    }
}
