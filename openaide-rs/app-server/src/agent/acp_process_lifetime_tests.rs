use std::time::Duration;

use super::ProcessIdleTimeouts;

/// The periodic Native Session catalog refresh runs every five minutes. List-only
/// retention must outlast it, or every refresh cold-starts each Agent process.
#[test]
fn list_only_retention_outlasts_the_catalog_refresh_interval() {
    let timeouts = ProcessIdleTimeouts::default();
    assert!(timeouts.short > Duration::from_secs(5 * 60));
    assert!(timeouts.long > timeouts.short);
}
