use super::*;

fn computer(id: &str, name: &str) -> RemoteComputer {
    RemoteComputer {
        id: id.to_string(),
        name: name.to_string(),
        ssh: None,
    }
}

#[test]
fn pairing_selects_the_computer_and_replaces_an_earlier_pairing_with_it() {
    let mut preferences = DesktopBootstrapPreferences::default();
    preferences.pair(computer("a", "Studio"));
    preferences.pair(computer("b", "Laptop"));
    preferences.pair(computer("a", "Studio again"));

    assert_eq!(preferences.remote_computers.len(), 2);
    assert_eq!(preferences.remote(), Some(&computer("a", "Studio again")));
}

#[test]
fn forgetting_the_computer_in_use_returns_to_this_computer() {
    let mut preferences = DesktopBootstrapPreferences::default();
    preferences.pair(computer("a", "Studio"));
    preferences.pair(computer("b", "Laptop"));

    assert!(!preferences.forget("a"));
    assert_eq!(preferences.remote(), Some(&computer("b", "Laptop")));
    assert!(preferences.forget("b"));
    assert_eq!(preferences.remote(), None);
    assert!(preferences.remote_computers.is_empty());
}

#[test]
fn preferences_written_before_pairing_existed_mean_this_computer() {
    let preferences: DesktopBootstrapPreferences =
        serde_json::from_str(r#"{"environment":{"kind":"native"}}"#).unwrap();

    assert_eq!(preferences.remote(), None);
    // A local-only installation keeps the file it always had.
    assert!(
        !serde_json::to_string(&preferences)
            .unwrap()
            .contains("remote")
    );
}

#[test]
fn a_selection_without_its_pairing_means_this_computer() {
    let preferences: DesktopBootstrapPreferences = serde_json::from_str(
        r#"{"environment":{"kind":"native"},"remoteComputer":"gone","remoteComputers":[]}"#,
    )
    .unwrap();

    assert_eq!(preferences.remote(), None);
}
