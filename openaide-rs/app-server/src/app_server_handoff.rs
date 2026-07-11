use std::path::{Path, PathBuf};

use openaide_app_server::app_server_client::launch_handoff::{
    AttachOrLaunchHandoff, LaunchHandoffResult, SleepLaunchWaiter,
};
use openaide_app_server::app_server_client::runner::{
    AttachOrLaunchRequirements, AttachOrLaunchRunner,
};
use openaide_app_server::app_server_client::{
    EndpointTarget, LocalHttpConnectionInfo, StorageWriterState,
};
use openaide_app_server::storage_runtime::{
    EndpointRecordStore, RuntimeLock, StateRoot, StateRootFingerprint, TransportKind,
};
use openaide_app_server_protocol::client::APP_SERVER_PROTOCOL_VERSION;

pub enum HandoffStart {
    Attached(LocalHttpHandoffConnection),
    LaunchHere(RuntimeLock),
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalHttpHandoffConnection {
    kind: &'static str,
    endpoint_url: String,
    auth_token: String,
}

impl From<LocalHttpConnectionInfo> for LocalHttpHandoffConnection {
    fn from(connection: LocalHttpConnectionInfo) -> Self {
        Self {
            kind: "localHttp",
            endpoint_url: connection.endpoint_url,
            auth_token: connection.auth_token,
        }
    }
}

pub fn attach_or_launch(state_root: &StateRoot, runtime_root: &Path) -> HandoffStart {
    let runner = AttachOrLaunchRunner::new(
        EndpointRecordStore::new(runtime_root),
        launch_lock_path(runtime_root, state_root.fingerprint()),
    );
    let handoff = AttachOrLaunchHandoff::new(runner, Default::default());
    let requirements = AttachOrLaunchRequirements {
        required_protocol_version: APP_SERVER_PROTOCOL_VERSION.to_string(),
        required_app_version: env!("CARGO_PKG_VERSION").to_string(),
    };
    let mut waiter = SleepLaunchWaiter::default();
    match handoff.run(
        state_root.fingerprint(),
        &requirements,
        StorageWriterState::Available,
        &mut waiter,
    ) {
        Ok(LaunchHandoffResult::AttachExisting { target }) => {
            HandoffStart::Attached(connection_from_target(&target).unwrap_or_else(|| {
                eprintln!("compatible OpenAIDE App Server has no LocalHttp endpoint");
                std::process::exit(1);
            }))
        }
        Ok(LaunchHandoffResult::LaunchNew { lock }) => HandoffStart::LaunchHere(lock),
        Ok(LaunchHandoffResult::Fail { reason }) => {
            eprintln!("OpenAIDE App Server handoff failed: {reason:?}");
            std::process::exit(1);
        }
        Err(error) => {
            eprintln!("OpenAIDE App Server handoff failed: {error}");
            std::process::exit(1);
        }
    }
}

pub fn print_connection_or_exit(connection: &LocalHttpHandoffConnection) {
    match serde_json::to_string(connection) {
        Ok(line) => println!("{line}"),
        Err(error) => {
            eprintln!("failed to serialize OpenAIDE App Server handoff: {error}");
            std::process::exit(1);
        }
    }
}

fn connection_from_target(target: &EndpointTarget) -> Option<LocalHttpHandoffConnection> {
    target
        .endpoints
        .iter()
        .find(|endpoint| endpoint.transport == TransportKind::LocalHttp)
        .map(|endpoint| LocalHttpHandoffConnection {
            kind: "localHttp",
            endpoint_url: endpoint.address.clone(),
            auth_token: target.auth_token.clone(),
        })
}

fn launch_lock_path(runtime_root: &Path, fingerprint: &StateRootFingerprint) -> PathBuf {
    runtime_root.join(format!("{}.launch.lock", fingerprint.as_str()))
}
