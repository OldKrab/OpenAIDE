use openaide_app_server::app_server_client::LocalHttpConnectionInfo;
use openaide_app_server::storage_runtime::{LockAcquireOutcome, RuntimeLock};

use super::*;
use crate::app_server_handoff::{IncompatibleServer, LocalHttpHandoffConnection};

fn connection() -> LocalHttpHandoffConnection {
    LocalHttpConnectionInfo {
        endpoint_url: "http://127.0.0.1:1".to_string(),
        auth_token: "token".to_string(),
    }
    .into()
}

#[test]
fn another_version_is_reported_and_not_joined() {
    let found = running_from(Ok(ReportingHandoff::Incompatible(IncompatibleServer {
        connection: connection(),
        app_version: "0.0.0".to_string(),
        protocol_version: "other".to_string(),
    })))
    .unwrap();

    assert!(matches!(
        found,
        Found::OtherVersion { protocol_version } if protocol_version == "other"
    ));
}

#[test]
fn a_compatible_server_is_joined_and_none_is_not_started() {
    let Found::Joined(joined) = running_from(Ok(ReportingHandoff::Attached(connection()))).unwrap()
    else {
        panic!("a compatible App Server must be joined");
    };
    assert_eq!(joined.endpoint_url, "http://127.0.0.1:1");
    assert!(!joined.started);

    let root = tempfile::tempdir().unwrap();
    let LockAcquireOutcome::Acquired(lock) =
        RuntimeLock::acquire(root.path().join("launch.lock")).unwrap()
    else {
        panic!("a fresh launch lock must be free");
    };
    assert!(matches!(
        running_from(Ok(ReportingHandoff::LaunchHere(lock))),
        Ok(Found::NotRunning)
    ));
    assert_eq!(
        running_from(Err("handoff_failed")).err(),
        Some("handoff_failed")
    );
}
