use super::*;

#[test]
fn a_line_yields_the_relay_and_its_password() {
    let relay = OwnRelay::parse("  https://relay.example.com:4443/?token=abc123 \n").unwrap();
    assert_eq!(relay.url().as_str(), "https://relay.example.com:4443/");
    assert_eq!(relay.host(), "relay.example.com");
    assert_eq!(relay.config().auth_token.as_deref(), Some("abc123"));
    assert_eq!(OwnRelay::parse(&relay.line()), Ok(relay));
}

#[test]
fn the_password_never_shows_in_debug_output() {
    let relay = OwnRelay::parse("https://relay.example.com/?token=secret-password").unwrap();
    let shown = format!("{relay:?}");
    assert!(shown.contains("relay.example.com"), "{shown}");
    assert!(!shown.contains("secret-password"), "{shown}");
}

#[test]
fn a_line_without_https_or_a_password_is_refused() {
    assert_eq!(
        OwnRelay::parse("relay.example.com"),
        Err(OwnRelayError::NotAnAddress)
    );
    assert_eq!(
        OwnRelay::parse("http://relay.example.com/?token=abc"),
        Err(OwnRelayError::NotHttps)
    );
    assert_eq!(
        OwnRelay::parse("https://relay.example.com/"),
        Err(OwnRelayError::NoPassword)
    );
    assert_eq!(
        OwnRelay::parse("https://relay.example.com/?token="),
        Err(OwnRelayError::NoPassword)
    );
}
