use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::*;

fn text(head: &TrustedHead) -> String {
    String::from_utf8(head.bytes.clone()).unwrap()
}

#[test]
fn the_device_credential_is_replaced_by_the_local_one() {
    let head = trust_head(
        b"POST /app HTTP/1.1\r\nHost: x\r\nAUTHORIZATION: Bearer guessed\r\nX-OpenAIDE-Connection-Id: conn-1\r\n\r\n",
        "local-token",
    )
    .unwrap();

    assert_eq!(
        text(&head),
        "POST /app HTTP/1.1\r\nAuthorization: Bearer local-token\r\nHost: x\r\nX-OpenAIDE-Connection-Id: conn-1\r\n\r\n"
    );
    assert_eq!(head.connection_id.as_deref(), Some("conn-1"));
}

#[test]
fn a_head_that_could_hide_a_second_credential_is_refused() {
    for head in [
        &b"POST /app HTTP/1.1\r\nX: a\nAuthorization: Bearer guessed\r\n\r\n"[..],
        b"POST /app HTTP/1.1\r\nAuthorization : Bearer guessed\r\n\r\n",
        b"POST /app HTTP/1.1\r\nno-colon\r\n\r\n",
        b"\r\n\r\n",
        b"POST /app HTTP/1.1\r\nX: \xff\r\n\r\n",
    ] {
        assert!(
            matches!(
                trust_head(head, "local-token"),
                Err(BridgeError::InvalidHead)
            ),
            "{head:?}"
        );
    }
}

#[tokio::test]
async fn a_request_reaches_the_local_listener_and_its_response_returns() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target = BridgeTarget {
        address: listener.local_addr().unwrap(),
        auth_token: "local-token".to_string(),
    };
    let local = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = vec![0u8; 256];
        let mut read = 0;
        while !request[..read].ends_with(b"ping") {
            read += socket.read(&mut request[read..]).await.unwrap();
        }
        socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\npong")
            .await
            .unwrap();
        String::from_utf8(request[..read].to_vec()).unwrap()
    });
    // The device keeps its sending half open, as a streaming client does.
    let (mut device, edge) = tokio::io::duplex(1024);
    let (edge_read, edge_write) = tokio::io::split(edge);
    device
        .write_all(b"POST /app HTTP/1.1\r\nContent-Length: 4\r\n\r\nping")
        .await
        .unwrap();

    let mut seen = None;
    forward(edge_read, edge_write, &target, |head| {
        seen = Some(head.connection_id.clone());
    })
    .await
    .unwrap();

    let mut response = Vec::new();
    device.read_to_end(&mut response).await.unwrap();
    assert!(response.ends_with(b"pong"));
    assert_eq!(seen, Some(None));
    let request = local.await.unwrap();
    assert!(request.starts_with("POST /app HTTP/1.1\r\nAuthorization: Bearer local-token\r\n"));
    assert!(request.ends_with("\r\n\r\nping"));
}

#[tokio::test]
async fn an_oversized_head_is_refused_before_reaching_the_local_listener() {
    let target = BridgeTarget {
        // Nothing listens here: reaching it would fail differently.
        address: "127.0.0.1:1".parse().unwrap(),
        auth_token: "local-token".to_string(),
    };
    let (mut device, edge) = tokio::io::duplex(64 * 1024);
    let (edge_read, edge_write) = tokio::io::split(edge);
    device
        .write_all(&vec![b'a'; MAX_HEAD_BYTES + 2048])
        .await
        .unwrap();

    let result = forward(edge_read, edge_write, &target, |_| {}).await;

    assert!(matches!(result, Err(BridgeError::InvalidHead)));
}
