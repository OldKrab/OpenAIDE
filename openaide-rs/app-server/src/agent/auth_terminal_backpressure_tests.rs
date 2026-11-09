use super::*;
use crate::client_lifecycle::ConnectionId;
use crate::server_requests::ServerRequestAnswer;

#[test]
fn a_login_which_does_not_read_stdin_remains_cancellable() {
    let temp = tempfile::TempDir::new().unwrap();
    let ready = temp.path().join("ready");
    let requests = ServerRequestRuntime::new();
    let client = ClientInstanceId::from("blocked-input-client");
    let runner = ClientAuthTerminal::new(
        requests.clone(),
        client.clone(),
        Delivery::new(
            client.clone(),
            ConnectionId::new("local-http:blocked-input-client"),
        ),
    );
    let cancel = CancelOnDrop::new();
    let token = cancel.token.clone();
    let ready_env = ready.to_string_lossy().into_owned();
    let (finished, completion) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let result = runner.run(
            AcpAgentConfig {
                agent_id: "fixture".into(),
                command: "sh".into(),
                args: vec![
                    "-c".into(),
                    "stty -icanon -echo; : > \"$AUTH_TEST_READY\"; sleep 30".into(),
                ],
                env: vec![("AUTH_TEST_READY".into(), ready_env)],
                secret_env: vec![],
            },
            token,
        );
        finished.send(result).unwrap();
    });
    // The marker is an explicit barrier: the process has disabled canonical input and is
    // deliberately not reading. Pasted bytes must not block the lifecycle worker.
    crate::test_sync::wait_until("the auth terminal process to be ready", || ready.exists());
    let mut responded = std::collections::HashSet::new();
    crate::test_sync::wait_until("control exchange despite blocked terminal I/O", || {
        for request in requests.pending_for_client(&client) {
            if !responded.insert(request.request_id.clone()) {
                continue;
            }
            requests.handle_response(
                client.clone(),
                request.request_id,
                ServerRequestAnswer::Result(
                    serde_json::to_value(ShellAuthTerminalResponse {
                        input: STANDARD.encode(vec![b'x'; 16384]),
                        cols: 80,
                        rows: 24,
                        cancel: false,
                    })
                    .unwrap(),
                ),
                AppServerTime::now(),
            );
        }
        responded.len() >= 2
    });
    cancel.token.cancel();
    assert!(completion
        .recv_timeout(crate::test_sync::WATCHDOG)
        .expect("cancel completes")
        .is_err());
    worker.join().unwrap();
}

#[test]
fn input_arriving_while_an_earlier_write_is_blocked_waits_for_the_login() {
    let temp = tempfile::TempDir::new().unwrap();
    let ready = temp.path().join("ready");
    let go = temp.path().join("go");
    let requests = ServerRequestRuntime::new();
    let client = ClientInstanceId::from("queued-input-client");
    let runner = ClientAuthTerminal::new(
        requests.clone(),
        client.clone(),
        Delivery::new(
            client.clone(),
            ConnectionId::new("local-http:queued-input-client"),
        ),
    );
    let gates = vec![
        (
            "AUTH_TEST_READY".into(),
            ready.to_string_lossy().into_owned(),
        ),
        ("AUTH_TEST_GO".into(), go.to_string_lossy().into_owned()),
    ];
    let worker = std::thread::spawn(move || {
        runner.run(
            AcpAgentConfig {
                agent_id: "fixture".into(),
                command: "sh".into(),
                // timing: poll — the login re-reads the release file the test creates.
                args: vec![
                    "-c".into(),
                    "stty -icanon -echo; : > \"$AUTH_TEST_READY\"; \
                     while [ ! -e \"$AUTH_TEST_GO\" ]; do sleep 0.01; done; \
                     head -c 16386 > /dev/null"
                        .into(),
                ],
                env: gates,
                secret_env: vec![],
            },
            TurnCancellation::new(),
        )
    });
    crate::test_sync::wait_until("the auth terminal process to be ready", || ready.exists());
    // The first batch overfills the PTY's input buffer, so its write blocks until the login
    // reads; the pasted code and Enter arrive behind it.
    let mut batches = vec![vec![b'x'; 16384], b"a".to_vec(), b"\n".to_vec()].into_iter();
    let mut responded = std::collections::HashSet::new();
    crate::test_sync::wait_until("the login to read every input batch", || {
        for request in requests.pending_for_client(&client) {
            if !responded.insert(request.request_id.clone()) {
                continue;
            }
            let input = batches.next().unwrap_or_default();
            requests.handle_response(
                client.clone(),
                request.request_id,
                ServerRequestAnswer::Result(
                    serde_json::to_value(ShellAuthTerminalResponse {
                        input: STANDARD.encode(input),
                        cols: 80,
                        rows: 24,
                        cancel: false,
                    })
                    .unwrap(),
                ),
                AppServerTime::now(),
            );
        }
        // A fourth exchange means the runner accepted all three batches.
        if responded.len() >= 4 && !go.exists() {
            std::fs::write(&go, "go").unwrap();
        }
        worker.is_finished()
    });
    assert!(worker.join().unwrap().is_ok());
}
