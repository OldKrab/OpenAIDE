use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;

use super::*;

fn store() -> (tempfile::TempDir, KeyStore) {
    let dir = tempfile::tempdir().expect("temporary directory");
    let store = KeyStore::open(dir.path());
    (dir, store)
}

#[test]
fn add_keeps_the_name_as_typed_and_issues_a_48_hex_token() {
    let (dir, store) = store();
    let olga = store.add("  Olga ", true).expect("added");
    assert_eq!(olga.name, "Olga");
    assert!(olga.can_manage);
    assert_eq!(olga.token.len(), 48);
    assert!(olga
        .token
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
    let pat = store.add("Pat", false).expect("added");
    assert_ne!(olga.token, pat.token);

    let names: Vec<_> = store
        .people()
        .expect("read")
        .iter()
        .map(|person| person.name.clone())
        .collect();
    assert_eq!(names, ["Olga", "Pat"], "creation order");
    let mode = std::fs::metadata(dir.path().join(KEYS_FILE))
        .expect("key file")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn a_name_is_unique_ignoring_case_and_spaces() {
    let (_dir, store) = store();
    store.add("Olga", false).expect("added");
    for duplicate in ["Olga", "olga", " OLGA "] {
        assert!(matches!(
            store.add(duplicate, false),
            Err(AddError::NameTaken)
        ));
    }
    assert_eq!(store.people().expect("read").len(), 1);
}

#[test]
fn a_name_has_1_to_40_characters_and_no_control_characters() {
    let (_dir, store) = store();
    for invalid in ["", "   ", "a\nb", "tab\there", "bell\u{7}", &"x".repeat(41)] {
        assert!(
            matches!(store.add(invalid, false), Err(AddError::InvalidName)),
            "{invalid:?}"
        );
    }
    store.add(&"é".repeat(40), false).expect("40 characters");
    store.add("Ольга", false).expect("any script");
}

#[test]
fn the_last_key_that_can_manage_cannot_be_revoked() {
    let (_dir, store) = store();
    store.add("owner", true).expect("added");
    store.add("Pat", false).expect("added");
    assert!(matches!(
        store.revoke("owner"),
        Err(RevokeError::LastManager)
    ));

    store.add("Olga", true).expect("added");
    store.revoke("OWNER").expect("another manager remains");
    assert!(matches!(
        store.revoke("Olga"),
        Err(RevokeError::LastManager)
    ));
    store.revoke("pat").expect("not a manager");
    assert!(matches!(
        store.revoke("Pat"),
        Err(RevokeError::NoSuchPerson)
    ));
    assert_eq!(store.people().expect("read").len(), 1);
}

#[test]
fn a_reader_sees_another_writers_change_without_reopening() {
    let (dir, reader) = store();
    let writer = KeyStore::open(dir.path());
    let olga = writer.add("Olga", true).expect("added");
    assert_eq!(
        reader.person_for_token(&olga.token).expect("read"),
        Some(olga.clone())
    );

    let pat = writer.add("Pat", false).expect("added");
    assert_eq!(
        reader
            .person_for_token(&pat.token)
            .expect("read")
            .map(|p| p.name),
        Some("Pat".to_string())
    );
    writer.revoke("Pat").expect("revoked");
    assert_eq!(reader.person_for_token(&pat.token).expect("read"), None);
    assert_eq!(reader.person_named("olga").expect("read"), Some(olga));

    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .expect("list")
        .map(|entry| entry.expect("entry").file_name())
        .filter(|name| name != KEYS_FILE)
        .collect();
    assert!(leftovers.is_empty(), "temporary files left: {leftovers:?}");
}

#[test]
fn concurrent_writers_lose_no_change() {
    let (dir, _store) = store();
    let writers: Vec<_> = (0..4)
        .map(|writer| {
            let store = Arc::new(KeyStore::open(dir.path()));
            std::thread::spawn(move || {
                for person in 0..10 {
                    store
                        .add(&format!("person {writer}-{person}"), false)
                        .expect("added");
                }
            })
        })
        .collect();
    for writer in writers {
        writer.join().expect("writer finished");
    }
    assert_eq!(KeyStore::open(dir.path()).people().expect("read").len(), 40);
}

#[test]
fn a_damaged_file_is_reported_and_left_alone() {
    let (dir, store) = store();
    let path = dir.path().join(KEYS_FILE);
    std::fs::write(&path, b"{ not json").expect("write");
    assert!(matches!(store.people(), Err(StoreError::Corrupt)));
    assert!(matches!(
        store.add("Olga", true),
        Err(AddError::Store(StoreError::Corrupt))
    ));
    assert_eq!(std::fs::read(&path).expect("read"), b"{ not json");
}

#[test]
fn a_wrong_token_matches_no_one() {
    let (_dir, store) = store();
    let olga = store.add("Olga", true).expect("added");
    let mut wrong = olga.token.clone();
    wrong.replace_range(47.., if wrong.ends_with('0') { "1" } else { "0" });
    assert_eq!(store.person_for_token(&wrong).expect("read"), None);
    assert_eq!(store.person_for_token("").expect("read"), None);
}

#[test]
fn debug_output_hides_the_token() {
    let (_dir, store) = store();
    let olga = store.add("Olga", true).expect("added");
    let text = format!("{olga:?}");
    assert!(!text.contains(&olga.token));
    assert!(!text.contains("Olga"));
    assert!(text.contains(&olga.key_id()));
}
