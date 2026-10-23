use super::quit_warning;

#[test]
fn quitting_with_terminals_open_on_this_computers_app_server_asks_first() {
    assert_eq!(
        quit_warning(1, true).as_deref(),
        Some("1 terminal will close.")
    );
    assert_eq!(
        quit_warning(3, true).as_deref(),
        Some("3 terminals will close.")
    );
}

#[test]
fn quitting_without_terminals_or_from_another_computer_does_not_ask() {
    assert_eq!(quit_warning(0, true), None);
    // The other computer's App Server keeps running, and its Terminals with it.
    assert_eq!(quit_warning(2, false), None);
}
