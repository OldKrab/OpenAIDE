use super::forwarded_head;

#[test]
fn a_request_is_carried_only_with_this_process_token_and_without_it() {
    let head = b"POST /probe HTTP/1.1\r\nHost: 127.0.0.1\r\nauthorization: Bearer secret\r\nX-A: b";
    let forwarded = String::from_utf8(forwarded_head(head, "secret").unwrap()).unwrap();
    assert_eq!(
        forwarded,
        "POST /probe HTTP/1.1\r\nHost: 127.0.0.1\r\nX-A: b\r\n\r\n"
    );
    assert_eq!(forwarded_head(head, "other"), Err(401));
    assert_eq!(
        forwarded_head(b"POST /probe HTTP/1.1\r\nX-A: b", "secret"),
        Err(401)
    );
}

#[test]
fn a_second_credential_cannot_ride_along() {
    let head =
        b"POST /probe HTTP/1.1\r\nAuthorization: Bearer secret\r\nAuthorization: Bearer other";
    assert_eq!(forwarded_head(head, "secret"), Err(401));
    let spaced = b"POST /probe HTTP/1.1\r\nAuthorization: Bearer secret\r\nAuthorization : x";
    assert_eq!(forwarded_head(spaced, "secret"), Err(400));
}

#[test]
fn a_preflight_passes_without_credentials() {
    let head = b"OPTIONS /probe HTTP/1.1\r\nOrigin: tauri://localhost";
    assert!(forwarded_head(head, "secret").is_ok());
}
