use super::*;

fn words(text: &str) -> Vec<String> {
    text.split_whitespace().map(str::to_string).collect()
}

#[test]
fn reads_each_command() {
    assert_eq!(parse(&words("")), Ok(Action::Run { pair: false }));
    assert_eq!(parse(&words("--pair")), Ok(Action::Run { pair: true }));
    assert_eq!(parse(&words("pair")), Ok(Action::Pair));
    assert_eq!(parse(&words("devices")), Ok(Action::Devices));
    assert_eq!(parse(&words("--help")), Ok(Action::Help));
    assert_eq!(
        parse(&words("remove 2")),
        Ok(Action::Remove {
            number: 2,
            confirmed: false
        })
    );
    assert_eq!(
        parse(&words("remove --yes 3")),
        Ok(Action::Remove {
            number: 3,
            confirmed: true
        })
    );
}

#[test]
fn refuses_what_it_cannot_read_rather_than_guessing() {
    for wrong in [
        "devcies",
        "pair now",
        "remove",
        "remove 0",
        "remove phone",
        "remove 1 2",
        "--pair x",
    ] {
        assert!(parse(&words(wrong)).is_err(), "{wrong}");
    }
}

#[test]
fn every_failure_has_a_sentence_and_a_stable_class() {
    let failures = [
        Failure::NotRunning,
        Failure::OtherVersion {
            protocol_version: "other".into(),
        },
        Failure::Handoff("handoff_failed"),
        Failure::Launch(launch::LaunchError::Spawn),
        Failure::Client(ClientError::Unreachable),
        Failure::Client(ClientError::Refused {
            code: "x".into(),
            message: "why".into(),
        }),
        Failure::RemoteAccess,
        Failure::NoSuchDevice(4),
        Failure::Cancelled,
        Failure::Stopped,
    ];
    for failure in failures {
        assert!(
            failure.message().ends_with('.') || failure.message().contains(": "),
            "{failure:?}"
        );
        assert!(failure
            .class()
            .chars()
            .all(|c| c.is_ascii_lowercase() || c == '_'));
    }
}
