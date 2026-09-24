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
    let deadline = Instant::now() + Duration::from_secs(5);
    // The marker is an explicit barrier: the process has disabled canonical input and is
    // deliberately not reading. Pasted bytes must not block the lifecycle worker.
    while !ready.exists() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    let mut responded = std::collections::HashSet::new();
    while responded.len() < 2 {
        assert!(
            Instant::now() < deadline,
            "terminal I/O blocked control exchange"
        );
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
        std::thread::sleep(Duration::from_millis(5));
    }
    cancel.token.cancel();
    assert!(completion
        .recv_timeout(Duration::from_secs(5))
        .expect("cancel completes")
        .is_err());
    worker.join().unwrap();
}
