use std::io::{Read, Write};
use std::net::TcpListener;

use iroh::address_lookup::MemoryLookup;
use openaide_app_server_protocol::devices::RemoteAccessState;

use super::*;
use crate::remote_devices::pairing_code::{InviteCode, JoinCode};
use crate::remote_devices::RemoteDevicesWorkflow;
use crate::test_sync::{wait_for, wait_until, WATCHDOG};

const LOCAL_TOKEN: &str = "local-token";

struct Fixture {
    _root: tempfile::TempDir,
    devices: RemoteDevices,
    lookup: MemoryLookup,
    runtime: tokio::runtime::Runtime,
}

/// An App Server edge on loopback, in front of a listener that answers every
/// request with the request head it received.
fn fixture() -> Fixture {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") && stream.read(&mut byte).unwrap_or(0) == 1 {
                head.push(byte[0]);
            }
            let _ = stream.write_all(
                format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", head.len()).as_bytes(),
            );
            let _ = stream.write_all(&head);
        }
    });
    let root = tempfile::tempdir().unwrap();
    let devices = RemoteDevices::open(root.path());
    let lookup = MemoryLookup::new();
    start_on(
        devices.clone(),
        BridgeTarget {
            address,
            auth_token: LOCAL_TOKEN.to_string(),
        },
        EdgeNetwork::Local(lookup.clone()),
    );
    Fixture {
        _root: root,
        devices,
        lookup,
        runtime: tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap(),
    }
}

impl Fixture {
    fn wait_for_access(&self, access: RemoteAccessState) {
        wait_until("remote access state", || {
            self.devices.snapshot().remote_access == access
        });
    }

    fn device_endpoint(&self, seed: u8, alpns: Vec<Vec<u8>>) -> Endpoint {
        let lookup = self.lookup.clone();
        self.run(async move {
            let endpoint = Endpoint::builder(presets::Minimal)
                .secret_key(SecretKey::from_bytes(&[seed; 32]))
                .alpns(alpns)
                .address_lookup(lookup.clone())
                .bind()
                .await
                .unwrap();
            lookup.add_endpoint_info(endpoint.addr());
            endpoint
        })
    }

    fn run<T>(&self, future: impl std::future::Future<Output = T>) -> T {
        self.runtime.block_on(async {
            tokio::time::timeout(WATCHDOG, future)
                .await
                .expect("watchdog expired: device side of the edge test")
        })
    }
}

async fn redeem(endpoint: &Endpoint, invite: &InviteCode, secret: &[u8]) -> PairingResult {
    let connection = endpoint.connect(invite.server, PAIRING_ALPN).await.unwrap();
    let (mut send, mut recv) = connection.open_bi().await.unwrap();
    let request = InviteRequest {
        secret: data_encoding::BASE32_NOPAD.encode(secret),
        name: "Phone".to_string(),
        model: Some("Model X".to_string()),
    };
    send.write_all(&serde_json::to_vec(&request).unwrap())
        .await
        .unwrap();
    send.finish().unwrap();
    let answer = recv.read_to_end(PAIRING_MESSAGE_LIMIT).await.unwrap();
    connection.close(0u32.into(), b"done");
    serde_json::from_slice(&answer).unwrap()
}

async fn request(connection: &Connection, head: &str) -> String {
    let (mut send, mut recv) = connection.open_bi().await.unwrap();
    send.write_all(head.as_bytes()).await.unwrap();
    send.finish().unwrap();
    String::from_utf8(recv.read_to_end(64 * 1024).await.unwrap()).unwrap()
}

#[test]
fn a_device_paired_by_invite_is_served_with_the_local_credential() {
    let fixture = fixture();
    let invite = fixture
        .devices
        .create_invite("desk".to_string(), AppServerTime::now().0)
        .unwrap();
    let invite = InviteCode::decode(&invite.code).unwrap();
    fixture.wait_for_access(RemoteAccessState::On);
    let device = fixture.device_endpoint(1, Vec::new());

    let (paired, response) = fixture.run(async {
        let paired = redeem(&device, &invite, &invite.secret).await;
        assert!(paired.trusted);
        let connection = device
            .connect(invite.server, &application_alpn())
            .await
            .unwrap();
        let response = request(
            &connection,
            "POST /app HTTP/1.1\r\nAuthorization: Bearer guessed\r\nX-OpenAIDE-Connection-Id: conn-1\r\n\r\n",
        )
        .await;
        (paired, (connection, response))
    });
    let (connection, response) = response;

    assert!(paired.trusted);
    assert_eq!(paired.server_name, Some(fixture.devices.server_name()));
    assert!(response.contains(&format!("Authorization: Bearer {LOCAL_TOKEN}\r\n")));
    assert!(!response.contains("guessed"));
    let summary = wait_for("the device to be listed as connected", || {
        fixture
            .devices
            .snapshot()
            .devices
            .into_iter()
            .find(|device| device.connection.is_some())
    });
    assert_eq!(summary.name, "Phone");
    assert_eq!(
        fixture
            .devices
            .device_name_for_connection("conn-1")
            .as_deref(),
        Some("Phone")
    );

    // Removal closes the live connection and stops listening for the last device.
    fixture.devices.remove(&summary.device_id).unwrap();
    fixture.run(connection.closed());
    fixture.wait_for_access(RemoteAccessState::Off);
}

#[test]
fn an_untrusted_key_is_refused_on_the_application_protocol() {
    let fixture = fixture();
    let invite = fixture
        .devices
        .create_invite("desk".to_string(), AppServerTime::now().0)
        .unwrap();
    let invite = InviteCode::decode(&invite.code).unwrap();
    fixture.wait_for_access(RemoteAccessState::On);
    let stranger = fixture.device_endpoint(9, Vec::new());

    let (wrong_secret, served) = fixture.run(async {
        let wrong_secret = redeem(&stranger, &invite, &[0; 16]).await;
        let connection = stranger
            .connect(invite.server, &application_alpn())
            .await
            .unwrap();
        // The refusal is a closed connection: no stream ever reaches a handler.
        connection.closed().await;
        let served = match connection.open_bi().await {
            Ok((mut send, mut recv)) => {
                let _ = send.write_all(b"POST /app HTTP/1.1\r\n\r\n").await;
                let _ = send.finish();
                recv.read_to_end(1024)
                    .await
                    .is_ok_and(|bytes| !bytes.is_empty())
            }
            Err(_) => false,
        };
        (wrong_secret, served)
    });

    assert!(!wrong_secret.trusted);
    assert!(!served);
    assert!(fixture.devices.snapshot().devices.is_empty());
}

#[test]
fn an_approved_join_request_is_announced_to_the_device() {
    let fixture = fixture();
    let device = fixture.device_endpoint(3, vec![PAIRING_ALPN.to_vec()]);
    let code = JoinCode {
        device: device.id(),
        name: "Tablet".to_string(),
        model: None,
    }
    .encode();

    fixture
        .devices
        .approve_join_request(&code, "desk".to_string(), 1)
        .unwrap();

    let (server, announced) = fixture.run(async {
        let connection = device.accept().await.unwrap().await.unwrap();
        let (mut send, mut recv) = connection.accept_bi().await.unwrap();
        let message = recv.read_to_end(PAIRING_MESSAGE_LIMIT).await.unwrap();
        send.finish().unwrap();
        connection.closed().await;
        (
            connection.remote_id(),
            serde_json::from_slice::<PairingResult>(&message).unwrap(),
        )
    });

    assert!(announced.trusted);
    assert_eq!(announced.server_name, Some(fixture.devices.server_name()));
    // The device learned the App Server's key from the connection itself.
    assert_eq!(
        Some(server),
        fixture
            .devices
            .endpoint_secret_key()
            .map(|key| key.public())
    );
    let response = fixture.run(async {
        let connection = device.connect(server, &application_alpn()).await.unwrap();
        request(&connection, "POST /app HTTP/1.1\r\n\r\n").await
    });
    assert!(response.contains(LOCAL_TOKEN));
}

fn quiet() -> openaide_remote_client::Logger {
    std::sync::Arc::new(|_, _| {})
}

/// One HTTP exchange with the Rust client's loopback gateway.
async fn through_gateway(address: std::net::SocketAddr, head: String) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
    socket.write_all(head.as_bytes()).await.unwrap();
    let mut response = String::new();
    socket.read_to_string(&mut response).await.unwrap();
    response
}

#[test]
fn the_rust_client_pairs_by_the_shown_code_and_carries_frontend_requests() {
    use openaide_remote_client::{Failure, Gateway, RemoteClient};

    let fixture = fixture();
    let shown = fixture
        .devices
        .create_invite("desk".to_string(), AppServerTime::now().0)
        .unwrap();
    fixture.wait_for_access(RemoteAccessState::On);
    let invite = openaide_remote_client::InviteCode::decode(&shown.code).unwrap();
    let endpoint = fixture.device_endpoint(5, Vec::new());
    let failures = std::sync::Arc::new(Mutex::new(Vec::<Failure>::new()));

    let (paired, gateway, served, refused) = fixture.run({
        let failures = failures.clone();
        let endpoint = endpoint.clone();
        async move {
            let paired = openaide_remote_client::redeem(
                &endpoint,
                &invite,
                "Laptop",
                Some("Desktop"),
                &quiet(),
            )
            .await
            .unwrap();
            let client = std::sync::Arc::new(RemoteClient::new(endpoint, paired.id, quiet()));
            let gateway = Gateway::start(client, quiet(), move |failure| {
                failures.lock().unwrap().push(failure)
            })
            .await
            .unwrap();
            let served = through_gateway(
                gateway.address,
                format!(
                    "POST /probe HTTP/1.1\r\nAuthorization: Bearer {}\r\nX-OpenAIDE-Connection-Id: conn-9\r\n\r\n",
                    gateway.token
                ),
            )
            .await;
            let refused = through_gateway(
                gateway.address,
                "POST /probe HTTP/1.1\r\nAuthorization: Bearer guessed\r\n\r\n".to_string(),
            )
            .await;
            (paired, gateway, served, refused)
        }
    });

    assert_eq!(paired.name, fixture.devices.server_name());
    assert!(served.starts_with("HTTP/1.1 200 OK"));
    assert!(served.contains(&format!("Authorization: Bearer {LOCAL_TOKEN}\r\n")));
    // The gateway's own token never leaves this computer.
    assert!(!served.contains(&gateway.token));
    assert!(refused.starts_with("HTTP/1.1 401"));
    assert_eq!(
        fixture
            .devices
            .device_name_for_connection("conn-9")
            .as_deref(),
        Some("Laptop")
    );

    // Once the computer removes this device, a retry cannot help and the shell is told.
    let device_id = fixture.devices.snapshot().devices[0].device_id.clone();
    fixture.devices.remove(&device_id).unwrap();
    wait_until("the removed device to be told", || {
        let head = format!(
            "POST /probe HTTP/1.1\r\nAuthorization: Bearer {}\r\n\r\n",
            gateway.token
        );
        let response = fixture.run(through_gateway(gateway.address, head));
        response.starts_with("HTTP/1.1 502")
            && failures.lock().unwrap().contains(&Failure::NotTrusted)
    });
}

#[test]
fn the_rust_client_names_an_app_server_of_another_protocol_major() {
    use openaide_remote_client::{Failure, RemoteClient};

    let fixture = fixture();
    let shown = fixture
        .devices
        .create_invite("desk".to_string(), AppServerTime::now().0)
        .unwrap();
    fixture.wait_for_access(RemoteAccessState::On);
    let invite = openaide_remote_client::InviteCode::decode(&shown.code).unwrap();
    let endpoint = fixture.device_endpoint(6, Vec::new());

    let result = fixture.run(async {
        let client = RemoteClient::with_alpn(
            endpoint,
            invite.server,
            b"openaide/app-server/9999".to_vec(),
            quiet(),
        );
        client.connection().await.map(|_| ())
    });

    assert_eq!(result, Err(Failure::Incompatible));
}

#[test]
fn the_rust_client_records_how_far_a_failed_connect_got() {
    use openaide_remote_client::{Failure, RemoteClient, SecretKey};

    let fixture = fixture();
    let endpoint = fixture.device_endpoint(7, Vec::new());
    let events = std::sync::Arc::new(Mutex::new(Vec::<(&'static str, serde_json::Value)>::new()));
    let log: openaide_remote_client::Logger = {
        let events = events.clone();
        std::sync::Arc::new(move |event, fields| events.lock().unwrap().push((event, fields)))
    };

    let result = fixture.run(async move {
        endpoint.close().await;
        let client = RemoteClient::new(endpoint, SecretKey::generate().public(), log);
        client.connection().await.map(|_| ())
    });

    assert_eq!(result, Err(Failure::Unreachable));
    let events = events.lock().unwrap();
    let (_, completed) = events
        .iter()
        .find(|(event, _)| *event == "remote_connect_completed")
        .expect("the attempt's terminal event");
    assert_eq!(completed["outcome"], "unreachable");
    assert_eq!(completed["stage"], "endpoint_closed");
    assert_eq!(completed["endpoint_closed"], true);
    assert!(completed["relay_connected"].is_boolean());
    assert!(completed["direct_addresses"].is_u64());
}

#[test]
fn a_stopped_gateway_refuses_its_port_and_leaves_the_app_server() {
    use openaide_remote_client::{Gateway, RemoteClient};

    let fixture = fixture();
    let shown = fixture
        .devices
        .create_invite("desk".to_string(), AppServerTime::now().0)
        .unwrap();
    fixture.wait_for_access(RemoteAccessState::On);
    let invite = openaide_remote_client::InviteCode::decode(&shown.code).unwrap();
    let endpoint = fixture.device_endpoint(6, Vec::new());
    // A shell keeps its endpoint for the life of the process, across switches.
    let _kept = endpoint.clone();

    let (address, served) = fixture.run(async move {
        let paired = openaide_remote_client::redeem(&endpoint, &invite, "Laptop", None, &quiet())
            .await
            .unwrap();
        let client = std::sync::Arc::new(RemoteClient::new(endpoint, paired.id, quiet()));
        let gateway = Gateway::start(client, quiet(), |_| {}).await.unwrap();
        let served = through_gateway(
            gateway.address,
            format!(
                "POST /probe HTTP/1.1\r\nAuthorization: Bearer {}\r\n\r\n",
                gateway.token
            ),
        )
        .await;
        let address = gateway.address;
        // Switching to another computer ends this one's gateway while the process lives on.
        gateway.stop().await;
        (address, served)
    });

    assert!(served.starts_with("HTTP/1.1 200 OK"));
    assert!(std::net::TcpStream::connect(address).is_err());
    wait_until("the App Server to see the device leave", || {
        fixture.devices.snapshot().devices[0].connection.is_none()
    });
}
