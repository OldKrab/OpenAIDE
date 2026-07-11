use super::*;
use std::sync::mpsc;

#[test]
fn config_options_times_out_when_worker_holds_reply_without_answer() {
    let (client, mut rx) = options_session_channel();
    let (seen_tx, seen_rx) = mpsc::channel();
    std::thread::spawn(move || {
        if let Some(AcpOptionsCommand::Get { reply_tx }) = rx.0.blocking_recv() {
            seen_tx.send(()).unwrap();
            std::thread::sleep(Duration::from_secs(1));
            drop(reply_tx);
        }
    });

    let error = client
        .config_options()
        .expect_err("held reply should time out");

    seen_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(error.to_string().contains("ACP options request timed out"));
}

#[test]
fn config_options_reports_stopped_worker_when_reply_is_dropped() {
    let (client, mut rx) = options_session_channel();
    std::thread::spawn(move || {
        if let Some(AcpOptionsCommand::Get { reply_tx }) = rx.0.blocking_recv() {
            drop(reply_tx);
        }
    });

    let error = client
        .config_options()
        .expect_err("dropped reply should fail");

    assert!(error
        .to_string()
        .contains("ACP options session worker stopped"));
}

#[test]
fn set_config_option_times_out_when_worker_holds_reply_without_answer() {
    let (client, mut rx) = options_session_channel();
    let (seen_tx, seen_rx) = mpsc::channel();
    std::thread::spawn(move || {
        if let Some(AcpOptionsCommand::Set { reply_tx, .. }) = rx.0.blocking_recv() {
            seen_tx.send(()).unwrap();
            std::thread::sleep(Duration::from_secs(1));
            drop(reply_tx);
        }
    });

    let error = client
        .set_config_option("model".to_string(), "gpt-5.5".to_string())
        .expect_err("held reply should time out");

    seen_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(error.to_string().contains("ACP options update timed out"));
}
