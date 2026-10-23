use super::*;

const SHELL: ProtocolVersion = ProtocolVersion { major: 2, minor: 2 };

#[test]
fn orders_releases_and_their_pre_releases() {
    assert_eq!(compare("0.10.0", "0.9.12"), Some(Ordering::Greater));
    assert_eq!(compare("0.10.0", "0.10.0+build.7"), Some(Ordering::Equal));
    assert_eq!(compare("0.11.0-rc.1", "0.11.0"), Some(Ordering::Less));
    assert_eq!(compare("0.11.0-rc.1", "0.11.0-rc.2"), None);
    assert_eq!(compare("0.11", "0.11.0"), None);
    assert_eq!(compare("dev", "0.11.0"), None);
}

#[test]
fn an_older_server_that_accepts_the_shell_protocol_can_be_joined() {
    assert_eq!(
        classify("0.11.0", SHELL, "0.10.1", Some("2.3")),
        Running::OlderCompatible
    );
    assert_eq!(
        classify("0.11.0", SHELL, "0.10.1", Some("2.1")),
        Running::OlderIncompatible
    );
    assert_eq!(
        classify("0.11.0", SHELL, "0.10.1", None),
        Running::OlderIncompatible
    );
}

#[test]
fn a_newer_or_unordered_server_is_never_replaced() {
    assert_eq!(
        classify("0.11.0", SHELL, "0.12.0", Some("2.2")),
        Running::Newer
    );
    assert_eq!(
        classify("0.11.0", SHELL, "custom", Some("2.2")),
        Running::Unknown
    );
    assert_eq!(
        classify("0.11.0", SHELL, "0.11.0", Some("2.2")),
        Running::Unknown
    );
}
