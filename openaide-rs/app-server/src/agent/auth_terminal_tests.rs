use super::*;
use crate::client_lifecycle::ConnectionId;
use crate::server_requests::{ResponseOutcome, ServerRequestAnswer};

#[test]
fn terminal_runs_configured_command_accepts_input_and_requires_zero_exit() {
    for exit in [0, 7] {
        let requests = ServerRequestRuntime::new();
        let client = ClientInstanceId::from("auth-client");
        let delivery = Delivery::new(client.clone(), ConnectionId::new("local-http:auth-client"));
        let runner = ClientAuthTerminal::new(requests.clone(), client.clone(), delivery);
        let worker = std::thread::spawn(move || {
            runner.run(AcpAgentConfig {
            agent_id: "fixture".into(), command: "sh".into(),
            args: vec!["-c".into(), format!("test -t 0 || exit 9; printf 'ready'; read value; test \"$value\" = supplied || exit 8; exit {exit}")],
            env: vec![], secret_env: vec![],
        }, TurnCancellation::new())
        });
        let mut sent_input = false;
        crate::test_sync::wait_until("the terminal exchange to finish", || {
            for pending in requests.pending_for_client(&client) {
                // The broker rejects another client even with the correct request id.
                assert!(matches!(
                    requests.handle_response(
                        ClientInstanceId::from("other"),
                        pending.request_id.clone(),
                        ServerRequestAnswer::Result(serde_json::json!({})),
                        AppServerTime::now()
                    ),
                    ResponseOutcome::UnauthorizedResponder { .. }
                ));
                let response = ShellAuthTerminalResponse {
                    input: if sent_input {
                        String::new()
                    } else {
                        STANDARD.encode(b"supplied\n")
                    },
                    cols: 90,
                    rows: 25,
                    cancel: false,
                };
                let result = requests.handle_response(
                    client.clone(),
                    pending.request_id,
                    ServerRequestAnswer::Result(serde_json::to_value(response).unwrap()),
                    AppServerTime::now(),
                );
                if matches!(result, ResponseOutcome::Accepted { .. }) {
                    sent_input = true;
                }
            }
            worker.is_finished()
        });
        assert_eq!(worker.join().unwrap().is_ok(), exit == 0);
        assert_eq!(requests.pending_count(), 0);
    }
}

#[test]
fn cancellation_releases_a_pending_terminal_exchange() {
    assert_terminal_interrupted(false);
}

#[test]
fn disconnect_releases_a_pending_terminal_exchange() {
    assert_terminal_interrupted(true);
}

fn assert_terminal_interrupted(disconnect: bool) {
    let requests = ServerRequestRuntime::new();
    let client = ClientInstanceId::from("auth-client");
    let runner = ClientAuthTerminal::new(
        requests.clone(),
        client.clone(),
        Delivery::new(client.clone(), ConnectionId::new("local-http:auth-client")),
    );
    let token = TurnCancellation::new();
    let worker_token = token.clone();
    let worker = std::thread::spawn(move || {
        runner.run(
            AcpAgentConfig {
                agent_id: "fixture".into(),
                command: "sh".into(),
                args: vec!["-c".into(), "read value".into()],
                env: vec![],
                secret_env: vec![],
            },
            worker_token,
        )
    });
    crate::test_sync::wait_until("the auth terminal request", || {
        !requests.pending_for_client(&client).is_empty()
    });
    if disconnect {
        requests.observe_transport_unavailable(&client, AppServerTime::now());
    } else {
        token.cancel();
    }
    assert!(worker.join().unwrap().is_err());
    assert_eq!(requests.pending_count(), 0);
}
