use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use openaide_app_server_protocol::devices::OwnRelayAccess;
use openaide_app_server_protocol::errors::ProtocolErrorCode;
use openaide_remote_core::{InviteCode, OwnRelay};

use super::super::relay_api::fake::FakeRelay;
use super::super::{RemoteDevices, RemoteDevicesWorkflow};

struct Fixture {
    _root: tempfile::TempDir,
    devices: RemoteDevices,
    relay: FakeRelay,
    published: Arc<AtomicU32>,
}

fn fixture() -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let relay = FakeRelay::default();
    let devices = RemoteDevices::open_with(root.path(), Arc::new(relay.clone()));
    devices.attach_edge(|_command| {});
    let published = Arc::new(AtomicU32::new(0));
    devices.on_changed({
        let published = published.clone();
        move || {
            published.fetch_add(1, Ordering::SeqCst);
        }
    });
    Fixture {
        _root: root,
        devices,
        relay,
        published,
    }
}

impl Fixture {
    fn invite_relay(&self) -> Option<OwnRelay> {
        let shown = self.devices.create_invite("desk".to_string(), 1).unwrap();
        InviteCode::decode(&shown.code).unwrap().relay
    }

    fn own_name(&self) -> String {
        self.devices.server_name()
    }
}

#[test]
fn a_manage_key_is_kept_for_managing_and_the_computer_relays_with_a_key_of_its_own() {
    let fixture = fixture();
    let manage = fixture.relay.issue("owner", true);

    fixture.devices.set_relay(Some(&manage)).unwrap();

    let shown = fixture.devices.snapshot().own_relay.unwrap();
    assert_eq!(shown.host, "relay.example.com");
    assert!(shown.can_manage);
    assert_eq!(shown.access, OwnRelayAccess::Accepted);
    assert_eq!(
        fixture.relay.names(),
        vec!["owner".to_string(), fixture.own_name()]
    );
    // A device gets the computer's ordinary key, never the one that can manage.
    let relayed = fixture.invite_relay().unwrap();
    assert_ne!(relayed, OwnRelay::parse(&manage).unwrap());
    assert_eq!(fixture.devices.own_relay(), Some(relayed));
}

#[test]
fn an_ordinary_key_relays_and_cannot_manage() {
    let fixture = fixture();
    let key = fixture.relay.issue("Olga", false);

    fixture.devices.set_relay(Some(&key)).unwrap();

    let shown = fixture.devices.snapshot().own_relay.unwrap();
    assert!(!shown.can_manage);
    assert_eq!(fixture.invite_relay(), OwnRelay::parse(&key).ok());
    assert_eq!(fixture.relay.names(), vec!["Olga".to_string()]);
    let refused = fixture.devices.relay_people().unwrap_err();
    assert_eq!(refused.code, ProtocolErrorCode::CapabilityUnavailable);
}

#[test]
fn a_key_the_relay_does_not_know_or_an_unreachable_relay_changes_nothing() {
    let fixture = fixture();
    let unknown = format!("{}?token=guessed", super::super::relay_api::fake::RELAY);
    let refused = fixture.devices.set_relay(Some(&unknown)).unwrap_err();
    assert_eq!(refused.code, ProtocolErrorCode::ValidationFailed);
    assert_eq!(refused.target.unwrap().field.as_deref(), Some("relay"));

    let key = fixture.relay.issue("Olga", false);
    fixture.relay.set_unreachable(true);
    assert!(fixture.devices.set_relay(Some(&key)).is_err());
    assert_eq!(fixture.devices.snapshot().own_relay, None);
}

#[test]
fn a_manager_adds_lists_and_revokes_people() {
    let fixture = fixture();
    fixture
        .devices
        .set_relay(Some(&fixture.relay.issue("owner", true)))
        .unwrap();

    let added = fixture.devices.relay_add_person(" Olga ", false).unwrap();
    assert_eq!(added.person.name, "Olga");
    assert!(!added.person.this_computer);
    assert!(OwnRelay::parse(&added.key).is_ok());
    let taken = fixture.devices.relay_add_person("olga", true).unwrap_err();
    assert_eq!(taken.code, ProtocolErrorCode::Conflict);

    let people = fixture.devices.relay_people().unwrap().people;
    let names: Vec<_> = people.iter().map(|person| person.name.as_str()).collect();
    assert_eq!(names, vec!["owner", fixture.own_name().as_str(), "Olga"]);
    assert!(people[1].this_computer);
    assert!(people[0].can_manage);

    fixture.devices.relay_revoke_person("Olga").unwrap();
    let own = fixture
        .devices
        .relay_revoke_person(&fixture.own_name())
        .unwrap_err();
    assert_eq!(own.code, ProtocolErrorCode::ValidationFailed);
    let last = fixture.devices.relay_revoke_person("owner").unwrap_err();
    assert_eq!(last.code, ProtocolErrorCode::Conflict);
    assert_eq!(
        fixture.relay.names(),
        vec!["owner".to_string(), fixture.own_name()]
    );
}

#[test]
fn the_computer_takes_a_free_name_and_gives_its_key_back_when_the_relay_changes() {
    let fixture = fixture();
    let manage = fixture.relay.issue("owner", true);
    fixture.relay.issue(&fixture.own_name(), false);

    fixture.devices.set_relay(Some(&manage)).unwrap();
    let own = format!("{} 2", fixture.own_name());
    assert!(fixture.relay.names().contains(&own));

    fixture.devices.set_relay(None).unwrap();
    assert!(
        !fixture.relay.names().contains(&own),
        "the computer's key is revoked"
    );
    assert_eq!(fixture.devices.snapshot().own_relay, None);
}

#[test]
fn a_revoked_computer_is_shown_as_refused() {
    let fixture = fixture();
    fixture
        .devices
        .set_relay(Some(&fixture.relay.issue("owner", true)))
        .unwrap();
    fixture.devices.check_relay_access();
    let quiet = fixture.published.load(Ordering::SeqCst);
    assert_eq!(quiet, 0, "an unchanged answer publishes nothing");

    fixture.relay.revoke_directly(&fixture.own_name());
    fixture.devices.check_relay_access();

    assert_eq!(fixture.published.load(Ordering::SeqCst), 1);
    let shown = fixture.devices.snapshot().own_relay.unwrap();
    assert_eq!(shown.access, OwnRelayAccess::Refused);

    // A new key clears the refusal.
    let key = fixture.relay.issue("Second", false);
    fixture.devices.set_relay(Some(&key)).unwrap();
    let shown = fixture.devices.snapshot().own_relay.unwrap();
    assert_eq!(shown.access, OwnRelayAccess::Accepted);
}

#[test]
fn the_same_key_twice_changes_nothing() {
    let fixture = fixture();
    let manage = fixture.relay.issue("owner", true);
    fixture.devices.set_relay(Some(&manage)).unwrap();
    let before = fixture.relay.names();

    let again = fixture
        .devices
        .set_relay(Some(&format!(" {manage} ")))
        .unwrap();

    assert_eq!(again.removed_devices, 0);
    assert_eq!(fixture.relay.names(), before);
}
