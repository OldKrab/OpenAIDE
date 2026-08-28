use url::Url;

use crate::desktop_update::{
    DesktopUpdateConfig, DesktopUpdateError, sha256_hex, trusted_redirect_url,
    validate_artifact_url,
};
use crate::desktop_update_receipt::{
    ReceiptOutcome, UpdateAttemptReceipt, classify_receipt, read_receipt, write_receipt,
};
use crate::desktop_update_schedule::{read_schedule, record_check_terminal};

#[test]
fn feed_endpoint_is_version_and_channel_specific() {
    let config = DesktopUpdateConfig {
        origin: Url::parse("https://updates.example.test/").unwrap(),
        release_public_key: "release-key".to_string(),
        recovery_public_key: Some("recovery-key".to_string()),
    };

    let stable = config.endpoint("1.2.3").unwrap().to_string();
    let prerelease = config.endpoint("1.3.0-beta.2").unwrap().to_string();

    assert!(stable.contains("/v1/stable/"));
    assert!(stable.ends_with("/1.2.3.json"));
    assert!(prerelease.contains("/v1/prerelease/"));
    assert!(prerelease.ends_with("/1.3.0-beta.2.json"));
}

#[test]
fn only_immutable_repository_release_urls_are_eligible() {
    assert!(
        validate_artifact_url(
            &Url::parse(
                "https://github.com/OldKrab/OpenAIDE/releases/download/v1.2.3/OpenAIDE.exe"
            )
            .unwrap()
        )
        .is_ok()
    );
    for rejected in [
        "http://github.com/OldKrab/OpenAIDE/releases/download/v1.2.3/OpenAIDE.exe",
        "https://github.com/OldKrab/Other/releases/download/v1.2.3/OpenAIDE.exe",
        "https://example.test/OldKrab/OpenAIDE/releases/download/v1.2.3/OpenAIDE.exe",
        "https://user@github.com/OldKrab/OpenAIDE/releases/download/v1.2.3/OpenAIDE.exe",
    ] {
        assert_eq!(
            validate_artifact_url(&Url::parse(rejected).unwrap()),
            Err(DesktopUpdateError::UntrustedArtifact)
        );
    }
}

#[test]
fn redirects_stay_on_the_feed_or_github_release_infrastructure() {
    assert!(trusted_redirect_url(
        &Url::parse("https://updates.example.test/v1/stable/windows/x86_64/1.0.0.json").unwrap(),
        "updates.example.test",
    ));
    assert!(trusted_redirect_url(
        &Url::parse("https://release-assets.githubusercontent.com/github-production-release-asset")
            .unwrap(),
        "updates.example.test",
    ));
    assert!(!trusted_redirect_url(
        &Url::parse("https://example.test/update.exe").unwrap(),
        "updates.example.test",
    ));
    assert!(!trusted_redirect_url(
        &Url::parse("https://user@github.com/OldKrab/OpenAIDE/releases/download/v1/app.exe")
            .unwrap(),
        "updates.example.test",
    ));
}

#[test]
fn receipt_proves_success_only_when_the_target_is_running() {
    let receipt = UpdateAttemptReceipt {
        previous_version: "1.0.0".to_string(),
        target_version: "1.1.0".to_string(),
        attempted_at_ms: 42,
    };

    assert_eq!(
        classify_receipt("1.1.0", Some(&receipt)),
        ReceiptOutcome::Updated("1.1.0".to_string())
    );
    assert_eq!(
        classify_receipt("1.0.0", Some(&receipt)),
        ReceiptOutcome::Incomplete
    );
    assert_eq!(
        classify_receipt("1.2.0", Some(&receipt)),
        ReceiptOutcome::Incomplete
    );
    assert_eq!(classify_receipt("1.0.0", None), ReceiptOutcome::None);
}

#[test]
fn receipt_commit_round_trips_without_artifact_metadata() {
    let directory =
        std::env::temp_dir().join(format!("openaide-update-test-{}", uuid::Uuid::new_v4()));
    let path = directory.join("receipt.json");
    let receipt = UpdateAttemptReceipt {
        previous_version: "1.0.0".to_string(),
        target_version: "1.1.0".to_string(),
        attempted_at_ms: 42,
    };

    write_receipt(&path, &receipt).unwrap();
    let restored = read_receipt(&path).unwrap();

    assert_eq!(restored.previous_version, "1.0.0");
    assert_eq!(restored.target_version, "1.1.0");
    assert_eq!(restored.attempted_at_ms, 42);
    let bytes = std::fs::read_to_string(&path).unwrap();
    assert!(!bytes.contains("url"));
    assert!(!bytes.contains("path"));
    assert!(!bytes.contains("signature"));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn artifact_digest_is_stable() {
    assert_eq!(
        sha256_hex(b"OpenAIDE"),
        "0b9279285b121aa0b38a184a0c232fef15b3cb0cfd952292658bb60966f64d43"
    );
}

#[test]
fn automatic_check_schedule_backs_off_and_resets_after_success() {
    let directory = std::env::temp_dir().join(format!(
        "openaide-update-schedule-test-{}",
        uuid::Uuid::new_v4()
    ));
    let path = directory.join("schedule.json");

    record_check_terminal(&path, 1_000_000, false);
    let failed = read_schedule(&path);
    assert_eq!(failed.failure_count, 1);
    assert!(failed.next_auto_check_at_ms > 1_000_000);

    record_check_terminal(&path, 2_000_000, true);
    let succeeded = read_schedule(&path);
    assert_eq!(succeeded.failure_count, 0);
    assert_eq!(succeeded.last_success_at_ms, Some(2_000_000));
    assert!(succeeded.next_auto_check_at_ms > 2_000_000 + 23 * 60 * 60 * 1_000);
    std::fs::remove_dir_all(directory).unwrap();
}
