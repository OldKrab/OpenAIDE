use std::time::Duration;

use super::{log_slow_chat_page, SLOW_CHAT_PAGE};

#[test]
fn only_a_slow_chat_page_is_logged_with_its_artifact_reads() {
    let logs = crate::logging::capture_test_logs();
    log_slow_chat_page(
        "task_fast_page",
        3,
        1,
        SLOW_CHAT_PAGE - Duration::from_millis(1),
    );
    log_slow_chat_page("task_slow_page", 421, 199, SLOW_CHAT_PAGE);

    let entries = logs.snapshot();
    let mine = entries
        .iter()
        .filter(|entry| entry["event"] == "chat_page_build_slow")
        .filter(|entry| {
            ["task_fast_page", "task_slow_page"]
                .contains(&entry["fields"]["task_id"].as_str().unwrap_or_default())
        })
        .collect::<Vec<_>>();
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0]["fields"]["task_id"], "task_slow_page");
    assert_eq!(mine[0]["fields"]["item_count"], 421);
    assert_eq!(mine[0]["fields"]["artifact_read_count"], 199);
}
