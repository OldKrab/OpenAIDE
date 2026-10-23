use crate::test_sync::wait_until;
use std::path::Path;

use openaide_app_server_protocol::errors::ProtocolErrorCode;
use openaide_app_server_protocol::ids::ProjectId;
use openaide_app_server_protocol::terminals::TerminalStatus;

use super::Terminals;

fn open(terminals: &Terminals) -> (String, String) {
    let opened = terminals
        .open(
            ProjectId::from("project".to_string()),
            Path::new("/"),
            80,
            24,
            "project",
        )
        .expect("shell starts");
    (opened.terminal.terminal_id, opened.attach.token)
}

#[test]
fn attach_token_is_single_use() {
    let terminals = Terminals::default();
    let (terminal_id, token) = open(&terminals);

    assert!(terminals.claim(&token).is_some());
    assert!(terminals.claim(&token).is_none());
    let renewed = terminals.attach(&terminal_id).unwrap();
    assert!(terminals.claim(&renewed.token).is_some());
    terminals.close_all();
}

#[test]
fn viewer_receives_output_and_a_later_viewer_receives_it_as_replay() {
    let terminals = Terminals::default();
    let (_, token) = open(&terminals);
    let shell = terminals.claim(&token).unwrap();
    let (_, first) = shell.attach();

    assert!(shell.write_input(b"echo openaide-$((40+2))\r".to_vec()));
    let mut seen = Vec::new();
    wait_until("the output arrived", || {
        while let Ok(chunk) = first.output.try_recv() {
            seen.extend(chunk);
        }
        String::from_utf8_lossy(&seen).contains("openaide-42")
    });
    let (_, later) = shell.attach();
    assert!(String::from_utf8_lossy(&later.replay).contains("openaide-42"));
    terminals.close_all();
}

#[test]
fn closing_a_busy_terminal_needs_confirmation() {
    let terminals = Terminals::default();
    let (terminal_id, token) = open(&terminals);
    let shell = terminals.claim(&token).unwrap();
    assert!(shell.write_input(b"sleep 30\r".to_vec()));
    wait_until("the shell runs a command", || shell.is_busy());

    let refused = terminals.close(&terminal_id, false).unwrap_err();
    assert_eq!(refused.code, ProtocolErrorCode::Conflict);
    terminals.close(&terminal_id, true).unwrap();
    assert!(terminals.snapshot().terminals.is_empty());
    assert_eq!(
        terminals.close(&terminal_id, true).unwrap_err().code,
        ProtocolErrorCode::NotFound
    );
}

#[test]
fn an_exited_shell_stays_listed_and_reports_changes() {
    let terminals = Terminals::default();
    let (changed_send, changed) = std::sync::mpsc::channel();
    terminals.on_changed(move || {
        let _ = changed_send.send(());
    });
    let (terminal_id, token) = open(&terminals);
    let shell = terminals.claim(&token).unwrap();
    assert!(shell.write_input(b"exit 3\r".to_vec()));

    wait_until("the shell exited", || {
        terminals.snapshot().terminals[0].status == TerminalStatus::Exited
    });
    let summary = &terminals.snapshot().terminals[0];
    assert_eq!(summary.exit_code, Some(3));
    assert!(changed.try_recv().is_ok());
    assert_eq!(terminals.open_count(), 0);
    terminals.close(&terminal_id, false).unwrap();
}

#[test]
fn rejects_an_out_of_range_size() {
    let terminals = Terminals::default();
    let refused = terminals
        .open(
            ProjectId::from("project".to_string()),
            Path::new("/"),
            1,
            24,
            "project",
        )
        .err()
        .unwrap();
    assert_eq!(refused.code, ProtocolErrorCode::ValidationFailed);
}
