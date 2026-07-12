use super::activity_millis;

#[test]
fn activity_time_normalizes_persisted_epochs_and_acp_utc_strings() {
    assert_eq!(
        activity_millis("1767225600000"),
        activity_millis("2026-01-01T00:00:00.000Z")
    );
}

#[test]
fn activity_time_normalizes_rfc3339_offset_timestamps() {
    let midnight_utc = activity_millis("2026-01-01T00:00:00.000Z");

    assert_eq!(activity_millis("2026-01-01T01:30:00+01:30"), midnight_utc);
    assert_eq!(activity_millis("2025-12-31T19:00:00-05:00"), midnight_utc);
}

#[test]
fn activity_time_preserves_fractional_milliseconds_with_an_offset() {
    assert_eq!(
        activity_millis("2026-01-01T01:30:00.123456+01:30"),
        activity_millis("2026-01-01T00:00:00.123Z")
    );
}

#[test]
fn activity_time_rejects_missing_or_unordered_values() {
    assert_eq!(activity_millis(""), None);
    assert_eq!(activity_millis("not-a-time"), None);
    assert_eq!(activity_millis("2026-02-30T00:00:00Z"), None);
    assert_eq!(activity_millis("2026-01-01T00:00:00.123noiseZ"), None);
    assert_eq!(activity_millis("2026-01-01T00:00:00+24:00"), None);
}
