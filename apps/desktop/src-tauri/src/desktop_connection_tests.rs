use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;

fn connection(port: u16) -> LocalHttpConnection {
    LocalHttpConnection {
        kind: "localHttp".to_string(),
        endpoint_url: format!("http://127.0.0.1:{port}/probe"),
        auth_token: "a".repeat(64),
    }
}

/// The window on this computer's App Server, as after a local start.
fn on_this_computer() -> DesktopConnections {
    let connections = DesktopConnections::default();
    connections.start_using(None, connection(1), None);
    connections
}

fn studio() -> Option<String> {
    Some("studio-key".to_string())
}

fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}

#[test]
fn a_switch_moves_the_window_with_a_new_client_instance() {
    let connections = on_this_computer();
    let (_, before) = connections.in_use().unwrap();
    let persisted = std::sync::Mutex::new(None);

    let outcome = block_on(connections.switch(
        studio(),
        "op-1",
        || async { Ok((connection(2), None)) },
        |target| {
            *persisted.lock().unwrap() = Some(target.clone());
            Ok(())
        },
    ));

    assert_eq!(outcome, Ok(SwitchOutcome::Switched));
    let (now, client_instance_id) = connections.in_use().unwrap();
    assert_eq!(now, connection(2));
    assert_ne!(client_instance_id, before);
    assert_eq!(connections.computer_in_use(), Some(studio()));
    assert_eq!(*persisted.lock().unwrap(), Some(studio()));
}

#[test]
fn the_computer_in_use_is_not_connected_again() {
    let connections = on_this_computer();
    let attempts = AtomicUsize::new(0);

    let outcome = block_on(connections.switch(
        None,
        "op-1",
        || async {
            attempts.fetch_add(1, Ordering::Relaxed);
            Ok((connection(2), None))
        },
        |_| Ok(()),
    ));

    assert_eq!(outcome, Ok(SwitchOutcome::AlreadyInUse));
    assert_eq!(attempts.load(Ordering::Relaxed), 0);
}

#[test]
fn a_failed_connection_leaves_the_window_where_it_was() {
    let connections = on_this_computer();
    let before = connections.in_use();

    let outcome = block_on(connections.switch(
        studio(),
        "op-1",
        || async {
            Err(ConnectError {
                class: "unreachable",
                message: "Studio is unreachable.".to_string(),
            })
        },
        |_| panic!("nothing is saved for a failed switch"),
    ));

    assert_eq!(
        outcome,
        Err(SwitchError::Failed("Studio is unreachable.".to_string()))
    );
    assert_eq!(connections.in_use(), before);
    assert_eq!(connections.computer_in_use(), Some(None));
}

#[test]
fn a_choice_that_cannot_be_saved_leaves_the_window_where_it_was() {
    let connections = on_this_computer();
    let before = connections.in_use();

    let outcome = block_on(connections.switch(
        studio(),
        "op-1",
        || async { Ok((connection(2), None)) },
        |_| Err("The choice could not be saved.".to_string()),
    ));

    assert_eq!(
        outcome,
        Err(SwitchError::Failed("The choice could not be saved.".to_string()))
    );
    assert_eq!(connections.in_use(), before);
}

#[test]
fn cancelling_ends_the_attempt_and_keeps_the_computer_in_use() {
    let connections = on_this_computer();
    let before = connections.in_use();

    let outcome = block_on(async {
        let switching = connections.switch(
            studio(),
            "op-1",
            || std::future::pending(),
            |_| panic!("nothing is saved for a cancelled switch"),
        );
        let cancelling = async {
            // The attempt registers itself before it waits on the target.
            while !connections.cancel() {
                tokio::task::yield_now().await;
            }
        };
        let (outcome, ()) = tokio::join!(switching, cancelling);
        outcome
    });

    assert_eq!(outcome, Err(SwitchError::Cancelled));
    assert_eq!(connections.in_use(), before);
    // Nothing is left to cancel once the attempt has ended.
    assert!(!connections.cancel());
}

#[test]
fn only_this_computers_app_server_replacement_reaches_the_window() {
    let connections = on_this_computer();
    assert!(connections.replace_local(connection(3)));
    assert_eq!(connections.in_use().unwrap().0, connection(3));
    assert_eq!(connections.local(), Some(connection(3)));

    let remote = DesktopConnections::default();
    remote.start_using(studio(), connection(2), None);
    assert!(!remote.replace_local(connection(4)));
    assert_eq!(remote.in_use().unwrap().0, connection(2));
    assert_eq!(remote.local(), Some(connection(4)));
}
