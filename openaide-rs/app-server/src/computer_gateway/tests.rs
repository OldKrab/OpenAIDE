use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;

use iroh::address_lookup::MemoryLookup;
use openaide_app_server_protocol::devices::RemoteAccessState;
use openaide_remote_client::{Logger, SecretKey};
use openaide_remote_core::Network;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};

use super::folder::{Computer, Folder};
use super::wire::{ComputerInUse, Event};
use super::{serve, ComputerGateway};
use crate::client_lifecycle::AppServerTime;
use crate::remote_devices::edge::start_on;
use crate::remote_devices::{BridgeTarget, RemoteDevices, RemoteDevicesWorkflow};
use crate::test_sync::{wait_until, WATCHDOG};

const LOCAL_TOKEN: &str = "local-token";

/// A Computer: an App Server edge on the in-memory network, in front of a
/// listener that answers every request with the request head it received.
struct RemoteComputer {
    _root: tempfile::TempDir,
    devices: RemoteDevices,
    lookup: MemoryLookup,
    runtime: tokio::runtime::Runtime,
}

fn remote_computer() -> RemoteComputer {
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
        Network::Local(lookup.clone()),
    );
    RemoteComputer {
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

impl RemoteComputer {
    fn code(&self) -> String {
        let code = self
            .devices
            .create_invite("desk".to_string(), AppServerTime::now().0)
            .unwrap()
            .code;
        wait_until("remote access", || {
            self.devices.snapshot().remote_access == RemoteAccessState::On
        });
        code
    }

    fn key(&self) -> String {
        self.devices.snapshot().server_key.unwrap()
    }

    /// The Web App's side: a gateway with its own folder, bound on this network.
    fn web_app(&self, folder: &std::path::Path) -> (Arc<ComputerGateway>, Events) {
        self.web_app_logged(folder, quiet())
    }

    fn web_app_logged(
        &self,
        folder: &std::path::Path,
        log: Logger,
    ) -> (Arc<ComputerGateway>, Events) {
        let network = Network::Local(self.lookup.clone());
        let endpoint = self.run(async move {
            let endpoint = network
                .builder()
                .secret_key(SecretKey::from_bytes(&[9; 32]))
                .alpns(Vec::new())
                .bind()
                .await
                .unwrap();
            network.on_bound(&endpoint);
            endpoint
        });
        let folder = Folder::open(folder).unwrap();
        let (gateway, events) =
            ComputerGateway::with_endpoint(folder, endpoint, "web-host".to_string(), log);
        (Arc::new(gateway), events)
    }

    fn run<T>(&self, future: impl std::future::Future<Output = T>) -> T {
        self.runtime.block_on(async {
            tokio::time::timeout(WATCHDOG, future)
                .await
                .expect("watchdog expired: Web App side of the gateway test")
        })
    }
}

type Events = tokio::sync::mpsc::UnboundedReceiver<Event>;

fn quiet() -> Logger {
    Arc::new(|_, _| {})
}

/// A logger that keeps the event names, and a way to count one of them.
fn recording() -> (Logger, impl Fn(&str) -> usize) {
    let events = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let log: Logger = {
        let events = events.clone();
        Arc::new(move |event, _| events.lock().unwrap().push(event.to_string()))
    };
    let count = move |name: &str| {
        events
            .lock()
            .unwrap()
            .iter()
            .filter(|event| *event == name)
            .count()
    };
    (log, count)
}

async fn through(endpoint_url: &str, token: &str) -> String {
    let address = endpoint_url
        .trim_start_matches("http://")
        .trim_end_matches("/probe");
    let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
    let head = format!("POST /probe HTTP/1.1\r\nAuthorization: Bearer {token}\r\n\r\n");
    socket.write_all(head.as_bytes()).await.unwrap();
    let mut response = String::new();
    socket.read_to_string(&mut response).await.unwrap();
    response
}

#[test]
fn a_spent_code_remembers_the_computer_and_its_requests_reach_it() {
    let computer = remote_computer();
    let folder = tempfile::tempdir().unwrap();
    let (gateway, _events) = computer.web_app(folder.path());
    let code = computer.code();

    let paired = computer.run(gateway.pair(&code, None)).unwrap();
    assert_eq!(paired.id, computer.key());
    assert_eq!(paired.name, computer.devices.server_name());
    assert_eq!(gateway.computers().unwrap(), vec![paired.clone()]);
    // The other Computer sees one device, named after the machine serving the Web App.
    let device = computer.devices.snapshot().devices[0].clone();
    assert_eq!(device.name, "web-host");
    assert_eq!(device.model.as_deref(), Some("OpenAIDE Web App"));

    let connection = computer.run(gateway.connect(&paired.id)).unwrap();
    assert_eq!(connection.kind, "localHttp");
    let served = computer.run(through(&connection.endpoint_url, &connection.auth_token));
    assert!(served.starts_with("HTTP/1.1 200 OK"), "{served}");
    assert!(served.contains(&format!("Authorization: Bearer {LOCAL_TOKEN}\r\n")));
    // Every browser on that Computer shares its one gateway.
    assert_eq!(
        computer.run(gateway.connect(&paired.id)).unwrap(),
        connection
    );
}

#[test]
fn the_code_of_the_computer_in_use_is_refused_before_it_is_spent() {
    let computer = remote_computer();
    let folder = tempfile::tempdir().unwrap();
    let (gateway, _events) = computer.web_app(folder.path());
    let code = computer.code();
    let in_use = ComputerInUse {
        key: computer.key(),
        name: "workstation".to_string(),
    };

    let refused = computer.run(gateway.pair(&code, Some(in_use))).unwrap_err();
    assert_eq!(refused.reason, "own_code");
    assert_eq!(
        refused.message,
        "This is workstation's own code. Enter it on the device that should use workstation."
    );
    // Still unspent, so the device it was made for can use it.
    assert!(computer.run(gateway.pair(&code, None)).is_ok());
}

#[test]
fn text_that_is_not_a_usable_code_is_refused_without_the_network() {
    let folder = tempfile::tempdir().unwrap();
    let (gateway, _events) = ComputerGateway::new(
        Folder::open(folder.path()).unwrap(),
        SecretKey::generate(),
        "web-host".to_string(),
        quiet(),
    );
    let runtime = tokio::runtime::Runtime::new().unwrap();
    for (text, reason) in [
        ("hello", "not_a_code"),
        ("OAI1AAAA", "malformed"),
        (
            "OAJ15VESRRRI2HBMN2XJAM4JAWMVMEUVSJZ2LRR7SNRWYFDBJLEHG7IQMVDBMJWGK5AA",
            "from_older_openaide",
        ),
    ] {
        let refused = runtime.block_on(gateway.pair(text, None)).unwrap_err();
        assert_eq!(refused.reason, reason, "{text}");
    }
    assert!(gateway.computers().unwrap().is_empty());
    assert_eq!(
        runtime
            .block_on(gateway.connect(&"a".repeat(64)))
            .unwrap_err()
            .reason,
        "not_paired"
    );
}

#[test]
fn a_removed_web_app_keeps_being_told_why() {
    let computer = remote_computer();
    let folder = tempfile::tempdir().unwrap();
    let (log, logged) = recording();
    let (gateway, mut events) = computer.web_app_logged(folder.path(), log);
    let paired = computer.run(gateway.pair(&computer.code(), None)).unwrap();
    let connection = computer.run(gateway.connect(&paired.id)).unwrap();

    let device_id = computer.devices.snapshot().devices[0].device_id.clone();
    computer.devices.remove(&device_id).unwrap();
    wait_until("the removal to reach the gateway", || {
        computer
            .run(through(&connection.endpoint_url, &connection.auth_token))
            .starts_with("HTTP/1.1 502")
    });
    assert_eq!(
        computer.run(events.recv()),
        Some(Event::Failure {
            computer: paired.id.clone(),
            reason: "not_trusted"
        })
    );
    let refused = computer.run(gateway.connect(&paired.id)).unwrap_err();
    assert_eq!(refused.reason, "not_trusted");
    assert!(refused.message.contains("removed"));
    // The event comes once per connection, not per refused request.
    assert!(events.try_recv().is_err());
    // Later requests are told without asking that Computer: a removed device stays
    // removed until it pairs again, and that App Server no longer even listens.
    let attempts = logged("remote_connect_started");
    assert_eq!(
        computer
            .run(gateway.connect(&paired.id))
            .unwrap_err()
            .reason,
        "not_trusted"
    );
    assert_eq!(logged("remote_connect_started"), attempts);

    // That App Server forgot its identity with its last device, so pairing again
    // makes a new Computer; the old one stays refused until the user forgets it.
    let again = computer.run(gateway.pair(&computer.code(), None)).unwrap();
    assert_ne!(again.id, paired.id);
    let reconnected = computer.run(gateway.connect(&again.id)).unwrap();
    assert!(computer
        .run(through(&reconnected.endpoint_url, &reconnected.auth_token))
        .starts_with("HTTP/1.1 200 OK"));
    assert_eq!(
        computer
            .run(gateway.connect(&paired.id))
            .unwrap_err()
            .reason,
        "not_trusted"
    );
}

#[test]
fn forgetting_a_computer_stops_its_gateway_and_removes_it() {
    let computer = remote_computer();
    let folder = tempfile::tempdir().unwrap();
    let (gateway, _events) = computer.web_app(folder.path());
    let paired = computer.run(gateway.pair(&computer.code(), None)).unwrap();
    let connection = computer.run(gateway.connect(&paired.id)).unwrap();

    computer.run(gateway.forget(&paired.id)).unwrap();
    let address = connection
        .endpoint_url
        .trim_start_matches("http://")
        .trim_end_matches("/probe")
        .to_string();
    assert!(std::net::TcpStream::connect(address).is_err());
    assert!(gateway.computers().unwrap().is_empty());
    assert_eq!(
        computer
            .run(gateway.connect(&paired.id))
            .unwrap_err()
            .reason,
        "not_paired"
    );
}

#[test]
fn a_computer_forgotten_while_a_browser_reconnects_is_not_reached_again() {
    let computer = remote_computer();
    let folder = tempfile::tempdir().unwrap();
    let (log, logged) = recording();
    let (gateway, _events) = computer.web_app_logged(folder.path(), log);
    let paired = computer.run(gateway.pair(&computer.code(), None)).unwrap();
    let attempts = logged("remote_connect_started");

    // The test holds the Computer's slot, as a connect under way does. Each task
    // waiting for the slot holds a reference to it, which is the gate.
    let slot = gateway.slot(&paired.id);
    let held = computer.run(slot.clone().lock_owned());
    let waiting = Arc::strong_count(&slot);
    let forgetting = computer.runtime.spawn({
        let gateway = gateway.clone();
        let id = paired.id.clone();
        async move { gateway.forget(&id).await }
    });
    wait_until("the forget waits for the slot", || {
        Arc::strong_count(&slot) == waiting + 1
    });
    // The forget stopping the gateway drops the browser's socket; the browser
    // asks again before the forget is done.
    let connecting = computer.runtime.spawn({
        let gateway = gateway.clone();
        let id = paired.id.clone();
        async move { gateway.connect(&id).await }
    });
    wait_until("the connect waits for the slot", || {
        Arc::strong_count(&slot) == waiting + 2
    });
    drop(held);

    computer.run(forgetting).unwrap().unwrap();
    let refused = computer.run(connecting).unwrap().unwrap_err();
    assert_eq!(refused.reason, "not_paired");
    assert_eq!(logged("remote_connect_started"), attempts);
    assert!(gateway.computers().unwrap().is_empty());
}

#[test]
fn a_connection_that_ended_is_opened_again_before_it_is_handed_out() {
    let computer = remote_computer();
    let folder = tempfile::tempdir().unwrap();
    let (log, logged) = recording();
    let (gateway, _events) = computer.web_app_logged(folder.path(), log);
    let paired = computer.run(gateway.pair(&computer.code(), None)).unwrap();
    let first = computer.run(gateway.connect(&paired.id)).unwrap();
    // The connection the running gateway keeps ends, as it does when the
    // Computer goes away.
    computer.run(async {
        let slot = gateway.slot(&paired.id);
        let slot = slot.lock().await;
        let client = &slot.running.as_ref().unwrap().client;
        client
            .connection()
            .await
            .unwrap()
            .close(0u32.into(), b"gone");
    });

    let attempts = logged("remote_connect_started");
    let again = computer.run(gateway.connect(&paired.id)).unwrap();
    assert_eq!(logged("remote_connect_started"), attempts + 1);
    assert_eq!(again, first, "the running gateway is handed out again");
}

#[test]
fn the_folder_keeps_its_key_and_list_private_and_across_restarts() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("web-app");
    let folder = Folder::open(&path).unwrap();
    let (key, created) = folder.key().unwrap();
    assert!(created);
    folder
        .remember(Computer {
            id: "a".repeat(64),
            name: "Studio".to_string(),
        })
        .unwrap();
    folder
        .remember(Computer {
            id: "a".repeat(64),
            name: "Studio again".to_string(),
        })
        .unwrap();

    let reopened = Folder::open(&path).unwrap();
    let (same, created) = reopened.key().unwrap();
    assert!(!created);
    assert_eq!(same.public(), key.public());
    assert_eq!(
        reopened.computers().unwrap(),
        vec![Computer {
            id: "a".repeat(64),
            name: "Studio again".to_string()
        }]
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode =
            |path: &std::path::Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o700);
        assert_eq!(mode(&path.join("device.key")), 0o600);
        assert_eq!(mode(&path.join("computers.json")), 0o600);
    }
    assert!(reopened.forget(&"a".repeat(64)).unwrap());
    assert!(!reopened.forget(&"a".repeat(64)).unwrap());
}

#[test]
fn the_process_answers_each_request_by_id_and_stops_when_its_input_ends() {
    let folder = tempfile::tempdir().unwrap();
    let (gateway, events) = ComputerGateway::new(
        Folder::open(folder.path()).unwrap(),
        SecretKey::generate(),
        "web-host".to_string(),
        quiet(),
    );
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let (mut shell, process_input) = tokio::io::duplex(4096);
        let (process_output, shell_output) = tokio::io::duplex(4096);
        let served = tokio::spawn(serve(
            Arc::new(gateway),
            events,
            tokio::io::BufReader::new(process_input),
            process_output,
        ));
        let mut lines = tokio::io::BufReader::new(shell_output).lines();
        assert_eq!(next(&mut lines).await, serde_json::json!({ "event": "ready" }));

        shell
            .write_all(b"not json\n{\"id\":7,\"method\":\"computers\"}\n{\"id\":8,\"method\":\"pair\",\"code\":\"hello\",\"inUse\":null}\n")
            .await
            .unwrap();
        let mut answers = [next(&mut lines).await, next(&mut lines).await];
        answers.sort_by_key(|answer| answer["id"].as_u64());
        assert_eq!(answers[0], serde_json::json!({ "id": 7, "result": { "computers": [] } }));
        assert_eq!(answers[1]["id"], 8);
        assert_eq!(answers[1]["error"]["reason"], "not_a_code");
        assert_eq!(answers[1]["error"]["message"], "This isn't an OpenAIDE code.");

        drop(shell);
        tokio::time::timeout(WATCHDOG, served).await.unwrap().unwrap();
    });
}

async fn next(
    lines: &mut tokio::io::Lines<tokio::io::BufReader<tokio::io::DuplexStream>>,
) -> serde_json::Value {
    let line = tokio::time::timeout(WATCHDOG, lines.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    serde_json::from_str(&line).unwrap()
}
