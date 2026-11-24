use std::io::{Read, Write};
use std::net::TcpListener;

use iroh::address_lookup::MemoryLookup;
use openaide_app_server_protocol::devices::RemoteAccessState;

use super::*;
use crate::remote_devices::RemoteDevicesWorkflow;
use crate::test_sync::{wait_for, wait_until, WATCHDOG};
use openaide_remote_core::{InviteCode, INVITE_SECRET_LEN};

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
    fixture_on(Network::Local(MemoryLookup::new()))
}

/// The same edge on `network`. On a network without the in-memory lookup, a device
/// is told the edge's address.
fn fixture_on(network: Network) -> Fixture {
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
    let lookup = match &network {
        Network::Local(lookup) => lookup.clone(),
        _ => MemoryLookup::new(),
    };
    start_on(
        devices.clone(),
        BridgeTarget {
            address,
            auth_token: LOCAL_TOKEN.to_string(),
        },
        network,
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
        let network = Network::Local(self.lookup.clone());
        self.run(async move {
            let endpoint = network
                .builder()
                .secret_key(SecretKey::from_bytes(&[seed; 32]))
                .alpns(alpns)
                .bind()
                .await
                .unwrap();
            network.on_bound(&endpoint);
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

/// Presents `secret` to the App Server that `invite` names, as a device spending it would.
async fn redeem(
    endpoint: &Endpoint,
    invite: &InviteCode,
    secret: &[u8; INVITE_SECRET_LEN],
) -> PairingResult {
    let connection = endpoint.connect(invite.server, PAIRING_ALPN).await.unwrap();
    let (mut send, mut recv) = connection.open_bi().await.unwrap();
    let request = InviteRequest {
        secret: InviteCode {
            server: invite.server,
            secret: *secret,
        }
        .secret_text(),
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

/// The edge's log lines seen so far. Other tests log at the same time, so a line is
/// looked up by its event and the device it names.
struct EdgeLog {
    capture: crate::logging::TestLogCapture,
    seen: Vec<serde_json::Value>,
}

impl EdgeLog {
    fn capture() -> Self {
        Self {
            capture: crate::logging::capture_test_logs(),
            seen: Vec::new(),
        }
    }

    /// The fields of the first `event` about `device`, waited for under the watchdog.
    fn fields(&mut self, event: &str, device: &Endpoint) -> serde_json::Value {
        let device = device.id().fmt_short().to_string();
        wait_for("the edge's log line", || {
            self.seen.extend(self.capture.snapshot());
            self.seen
                .iter()
                .find(|line| line["event"] == event && line["fields"]["device"] == device)
                .map(|line| line["fields"].clone())
        })
    }
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

#[test]
fn a_transport_failure_reports_its_number_without_its_reason() {
    let failure = ConnectionError::TransportError(iroh::endpoint::TransportError::new(
        iroh::endpoint::TransportErrorCode::crypto(120),
        "peer text".to_string(),
    ));

    assert_eq!(transport_code(&failure), Some(0x100 + 120));
    assert_eq!(transport_code(&ConnectionError::TimedOut), None);
}

#[test]
fn connection_failures_keep_a_stable_kind() {
    assert_eq!(
        connection_error_kind(&ConnectionError::TimedOut),
        "timed_out"
    );
    assert_eq!(connection_error_kind(&ConnectionError::Reset), "reset");
    assert_eq!(
        connection_error_kind(&ConnectionError::VersionMismatch),
        "version_mismatch"
    );
}

#[test]
fn a_handshake_failure_names_its_route_and_only_a_relayed_sender() {
    let root = tempfile::tempdir().unwrap();
    let devices = RemoteDevices::open(root.path());
    let sender = SecretKey::from_bytes(&[7; 32]).public();

    let relayed = IncomingAddr::Relay {
        url: "https://relay.example".parse().unwrap(),
        endpoint_id: sender,
    };
    assert_eq!(
        IncomingOrigin::of(&relayed, &devices),
        IncomingOrigin {
            route: "relayed",
            device: Some(sender.fmt_short().to_string()),
            trusted: Some(false),
        }
    );

    let direct = IncomingAddr::Ip("192.0.2.1:4433".parse().unwrap());
    assert_eq!(
        IncomingOrigin::of(&direct, &devices),
        IncomingOrigin {
            route: "direct",
            device: None,
            trusted: None
        }
    );
}

#[test]
fn a_served_connection_logs_its_handshake_its_bytes_and_how_it_ended() {
    let fixture = fixture();
    let invite = fixture
        .devices
        .create_invite("desk".to_string(), AppServerTime::now().0)
        .unwrap();
    let invite = InviteCode::decode(&invite.code).unwrap();
    fixture.wait_for_access(RemoteAccessState::On);
    let device = fixture.device_endpoint(21, Vec::new());
    let mut log = EdgeLog::capture();
    let head = format!(
        "POST /app HTTP/1.1\r\nX-Padding: {}\r\n\r\n",
        "p".repeat(4000)
    );

    let response = fixture.run(async {
        assert!(redeem(&device, &invite, &invite.secret).await.trusted);
        let connection = device
            .connect(invite.server, &application_alpn())
            .await
            .unwrap();
        let response = request(&connection, &head).await;
        connection.close(7u32.into(), b"bye");
        response
    });

    let connected = log.fields("remote_device_connected", &device);
    assert_eq!(connected["route"], "direct");
    assert!(connected["handshake_ms"].is_u64(), "{connected}");
    let ended = log.fields("remote_device_disconnected", &device);
    assert_eq!(ended["close_kind"], "application_closed");
    assert_eq!(ended["close_code"], 7);
    assert_eq!(ended["streams"], 1);
    assert!(
        ended["tx_bytes"].as_u64().unwrap() >= response.len() as u64,
        "{ended}"
    );
    assert!(
        ended["rx_bytes"].as_u64().unwrap() >= head.len() as u64,
        "{ended}"
    );
    assert_eq!(ended["relayed_tx_bytes"], 0);
    assert_eq!(ended["relayed_rx_bytes"], 0);
    // The closing reason a device sends is free-form and stays out of the log.
    assert!(!ended.to_string().contains("bye"));
}

#[test]
fn a_connection_through_a_relay_logs_the_bytes_the_relay_carried() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let (map, relay_url, _relay) = runtime
        .block_on(iroh::test_utils::run_relay_server())
        .unwrap();
    let network = Network::Relay(map);
    let fixture = fixture_on(network.clone());
    let invite = fixture
        .devices
        .create_invite("desk".to_string(), AppServerTime::now().0)
        .unwrap();
    let invite = InviteCode::decode(&invite.code).unwrap();
    fixture.wait_for_access(RemoteAccessState::On);
    let device = fixture.run(async {
        network
            .builder()
            .secret_key(SecretKey::from_bytes(&[22; 32]))
            .bind()
            .await
            .unwrap()
    });
    // The relay network has no lookup, so the device is told where the edge is,
    // and trusted without the pairing exchange.
    fixture
        .devices
        .redeem_invite(
            &device.id(),
            &invite.secret,
            "Phone",
            None,
            AppServerTime::now().0,
        )
        .unwrap();
    let edge = iroh::EndpointAddr::new(invite.server).with_relay_url(relay_url);
    let mut log = EdgeLog::capture();

    fixture.run(async {
        let connection = device.connect(edge, &application_alpn()).await.unwrap();
        request(&connection, "POST /app HTTP/1.1\r\n\r\n").await;
        connection.close(0u32.into(), b"done");
    });

    assert_eq!(
        log.fields("remote_device_connected", &device)["route"],
        "relayed"
    );
    let ended = log.fields("remote_device_disconnected", &device);
    let relayed = ended["relayed_rx_bytes"].as_u64().unwrap();
    // The device knew only the relay, so its handshake arrived through it.
    assert!(relayed > 0, "{ended}");
    assert!(relayed <= ended["rx_bytes"].as_u64().unwrap(), "{ended}");
    assert!(
        ended["relayed_tx_bytes"].as_u64().unwrap() <= ended["tx_bytes"].as_u64().unwrap(),
        "{ended}"
    );
}
