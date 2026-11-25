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
    assert_eq!(parse(&words("relay")), Ok(Action::Relay));
    assert_eq!(
        parse(&words("relay set")),
        Ok(Action::SetRelay { confirmed: false })
    );
    assert_eq!(
        parse(&words("relay clear --yes")),
        Ok(Action::ClearRelay { confirmed: true })
    );
}

#[test]
fn a_relay_line_is_never_taken_from_the_arguments() {
    for wrong in [
        "relay set https://relay.example.com/?token=secret",
        "relay https://relay.example.com/?token=secret",
    ] {
        let problem = parse(&words(wrong)).unwrap_err();
        assert!(problem.contains("standard input"), "{problem}");
        assert!(!problem.contains("secret"), "{problem}");
    }
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
        "relay sett",
        "relay set now",
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
        Failure::BadRelay(openaide_remote_core::OwnRelayError::NoPassword),
        Failure::RelayNeedsConfirmation(2),
        Failure::RelayKept,
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
