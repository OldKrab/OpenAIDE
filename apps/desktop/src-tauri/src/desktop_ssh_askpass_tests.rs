use super::*;

#[test]
fn prompts_are_classified_by_hint_and_wording() {
    assert_eq!(
        PromptKind::of("someone@host's password: ", None),
        PromptKind::Secret
    );
    assert_eq!(
        PromptKind::of(
            "Are you sure you want to continue connecting (yes/no/[fingerprint])? ",
            None
        ),
        PromptKind::YesNo
    );
    assert_eq!(
        PromptKind::of("Allow use of key?", Some("confirm")),
        PromptKind::YesNo
    );
    assert_eq!(
        PromptKind::of("Confirm user presence for key", Some("none")),
        PromptKind::Notice
    );
}

#[test]
fn only_the_handed_out_token_is_accepted() {
    assert!(same_token("abc123", "abc123"));
    assert!(!same_token("abc124", "abc123"));
    assert!(!same_token("abc12", "abc123"));
}
