use super::loopback_address;

#[test]
fn accepts_only_loopback_app_server_endpoints() {
    assert_eq!(
        loopback_address("http://127.0.0.1:43123/rpc")
            .expect("loopback endpoint")
            .port(),
        43123
    );
    assert!(loopback_address("https://example.com/rpc").is_none());
    assert!(loopback_address("not-a-url").is_none());
}
