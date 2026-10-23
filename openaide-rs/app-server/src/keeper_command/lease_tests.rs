use super::*;

fn holder(server_id: &str) -> LeaseHolder {
    LeaseHolder {
        pid: 4242,
        server_id: server_id.to_string(),
    }
}

#[test]
fn a_second_keeper_sees_the_first_one_and_its_server() {
    let dir = tempfile::tempdir().unwrap();
    let path = lease_path(dir.path(), "root-1");

    let first = try_acquire(&path, &holder("server-a")).unwrap();
    let second = try_acquire(&path, &holder("server-b")).unwrap();

    assert!(matches!(first, LeaseAttempt::Acquired(_)));
    assert!(matches!(
        second,
        LeaseAttempt::Held(Some(ref found)) if *found == holder("server-a")
    ));
    assert_eq!(current_holder(&path).unwrap(), Some(holder("server-a")));
}

#[test]
fn a_released_lease_is_free_whatever_the_file_still_says() {
    let dir = tempfile::tempdir().unwrap();
    let path = lease_path(dir.path(), "root-1");
    drop(try_acquire(&path, &holder("server-a")).unwrap());

    assert_eq!(current_holder(&path).unwrap(), None);
    let second = try_acquire(&path, &holder("server-b")).unwrap();

    assert!(matches!(second, LeaseAttempt::Acquired(_)));
    assert_eq!(current_holder(&path).unwrap(), Some(holder("server-b")));
}

#[test]
fn no_lease_file_means_no_keeper() {
    let dir = tempfile::tempdir().unwrap();

    assert_eq!(
        current_holder(&lease_path(dir.path(), "root-1")).unwrap(),
        None
    );
}
