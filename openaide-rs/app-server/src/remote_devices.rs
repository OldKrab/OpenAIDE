//! Remote Devices: App Shells on other machines that the App Server trusts by key (ADR-0062).
//!
//! This module owns the durable trust list, the pending invite, and the live
//! connection state. The network edge in [`edge`] asks it who is trusted and
//! reports what connected; the protocol gateway asks it to pair and remove.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use iroh::{PublicKey, SecretKey};
use openaide_app_server_protocol::devices::{
    DeviceCollectionSnapshot, DevicesCreateInviteResult, DevicesPreviewJoinRequestResult,
    RemoteAccessState, RemoteDeviceConnection, RemoteDeviceSummary,
};
use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};

mod bridge;
pub mod edge;
mod host_name;
mod pairing_code;
mod store;

use pairing_code::{clamp_label, InviteCode, JoinCode, PairingCodeError, INVITE_SECRET_LEN};
use store::{RemoteDeviceStore, StoredRemoteDevice, StoredRemoteDevices};

/// An invite is refreshed by the client that shows it; this bounds one left open.
const INVITE_LIFETIME_MS: u64 = 10 * 60 * 1000;

/// The pairing and trust decisions the protocol gateway exposes to clients.
pub(crate) trait RemoteDevicesWorkflow: Send + Sync {
    fn snapshot(&self) -> DeviceCollectionSnapshot;
    fn create_invite(
        &self,
        added_by: String,
        now_ms: u64,
    ) -> Result<DevicesCreateInviteResult, ProtocolError>;
    fn cancel_invite(&self);
    fn preview_join_request(
        &self,
        code: &str,
    ) -> Result<DevicesPreviewJoinRequestResult, ProtocolError>;
    fn approve_join_request(
        &self,
        code: &str,
        added_by: String,
        now_ms: u64,
    ) -> Result<(), ProtocolError>;
    fn remove(&self, device_id: &str) -> Result<(), ProtocolError>;
    /// The Remote Device behind a protocol connection, or `None` for a same-machine client.
    fn device_name_for_connection(&self, connection_id: &str) -> Option<String>;
}

/// Work the network edge performs on behalf of a trust decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EdgeCommand {
    /// Start or stop the endpoint to match whether remote access is in use.
    Reconcile,
    /// Tell a device approved from its join request which App Server trusts it.
    AnnounceTrust(PublicKey),
    /// Close every connection of a removed device.
    Disconnect(PublicKey),
}

/// Why an invite presented by a new device was not accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InviteRejection {
    NoInvite,
    Expired,
    WrongSecret,
    Storage,
}

impl InviteRejection {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::NoInvite => "no_invite",
            Self::Expired => "expired",
            Self::WrongSecret => "wrong_secret",
            Self::Storage => "storage",
        }
    }
}

#[derive(Clone)]
pub struct RemoteDevices {
    inner: Arc<Inner>,
}

struct Inner {
    store: RemoteDeviceStore,
    server_name: String,
    state: Mutex<State>,
}

struct State {
    stored: StoredRemoteDevices,
    invite: Option<Invite>,
    access: RemoteAccessState,
    connections: HashMap<String, ConnectedDevice>,
    /// Protocol connection ids seen on a device's streams, to label what it pairs.
    connection_devices: HashMap<String, String>,
    edge: Option<Box<dyn Fn(EdgeCommand) + Send>>,
    /// Called without the state lock after a change the edge made.
    changed: Option<Arc<dyn Fn() + Send + Sync>>,
}

struct Invite {
    secret: [u8; INVITE_SECRET_LEN],
    added_by: String,
    expires_at_ms: u64,
}

struct ConnectedDevice {
    connection: RemoteDeviceConnection,
    /// A device may hold several connections while it changes networks.
    open_connections: usize,
}

impl RemoteDevices {
    /// A damaged trust file is reported and remote access stays off: failing closed
    /// keeps a corrupted file from turning into trust nobody granted.
    pub fn open(state_root: &Path) -> Self {
        let store = RemoteDeviceStore::new(state_root);
        let stored = store.read().unwrap_or_else(|_error| {
            crate::logging::error(
                "remote_devices_store_unreadable",
                serde_json::json!({ "outcome": "remote_access_disabled" }),
            );
            StoredRemoteDevices::default()
        });
        Self {
            inner: Arc::new(Inner {
                store,
                server_name: host_name::host_name(),
                state: Mutex::new(State {
                    stored,
                    invite: None,
                    access: RemoteAccessState::Off,
                    connections: HashMap::new(),
                    connection_devices: HashMap::new(),
                    edge: None,
                    changed: None,
                }),
            }),
        }
    }

    /// Lets the protocol gateway republish the collection after an edge-side change.
    pub fn on_changed(&self, changed: impl Fn() + Send + Sync + 'static) {
        self.state().changed = Some(Arc::new(changed));
    }

    pub(crate) fn attach_edge(&self, edge: impl Fn(EdgeCommand) + Send + 'static) {
        let mut state = self.state();
        state.edge = Some(Box::new(edge));
        state.send(EdgeCommand::Reconcile);
    }

    pub(crate) fn server_name(&self) -> String {
        self.inner.server_name.clone()
    }

    /// The identity to listen with while remote access is in use.
    pub(crate) fn endpoint_secret_key(&self) -> Option<SecretKey> {
        let state = self.state();
        state
            .wants_endpoint()
            .then(|| state.stored.secret_key())
            .flatten()
    }

    pub(crate) fn is_trusted(&self, device: &PublicKey) -> bool {
        let device_id = device.to_string();
        self.state()
            .stored
            .devices
            .iter()
            .any(|stored| stored.device_id == device_id)
    }

    pub(crate) fn set_access(&self, access: RemoteAccessState) {
        let changed = {
            let mut state = self.state();
            if state.access == access {
                return;
            }
            state.access = access;
            state.changed.clone()
        };
        notify(changed);
    }

    /// Trusts the device that presented the pending invite's secret, and spends the invite.
    pub(crate) fn redeem_invite(
        &self,
        device: &PublicKey,
        secret: &[u8],
        name: &str,
        model: Option<&str>,
        now_ms: u64,
    ) -> Result<(), InviteRejection> {
        let changed = {
            let mut state = self.state();
            let invite = state.invite.as_ref().ok_or(InviteRejection::NoInvite)?;
            if now_ms >= invite.expires_at_ms {
                state.invite = None;
                return Err(InviteRejection::Expired);
            }
            if !constant_time_eq(&invite.secret, secret) {
                return Err(InviteRejection::WrongSecret);
            }
            let added_by = invite.added_by.clone();
            let mut stored = state.stored.clone();
            trust(&mut stored, device, name, model, added_by, now_ms);
            self.inner
                .store
                .write(&stored)
                .map_err(|_error| InviteRejection::Storage)?;
            state.stored = stored;
            state.invite = None;
            state.changed.clone()
        };
        notify(changed);
        Ok(())
    }

    pub(crate) fn device_connected(&self, device: &PublicKey, connection: RemoteDeviceConnection) {
        let changed = {
            let mut state = self.state();
            let entry = state
                .connections
                .entry(device.to_string())
                .or_insert(ConnectedDevice {
                    connection: connection.clone(),
                    open_connections: 0,
                });
            entry.open_connections += 1;
            entry.connection = connection;
            state.changed.clone()
        };
        notify(changed);
    }

    pub(crate) fn device_path_changed(
        &self,
        device: &PublicKey,
        connection: RemoteDeviceConnection,
    ) {
        let changed = {
            let mut state = self.state();
            let Some(entry) = state.connections.get_mut(&device.to_string()) else {
                return;
            };
            if entry.connection == connection {
                return;
            }
            entry.connection = connection;
            state.changed.clone()
        };
        notify(changed);
    }

    /// Records when a device was last reachable once its final connection closes.
    pub(crate) fn device_disconnected(&self, device: &PublicKey, now_ms: u64) {
        let changed = {
            let mut state = self.state();
            let device_id = device.to_string();
            let Some(entry) = state.connections.get_mut(&device_id) else {
                return;
            };
            entry.open_connections = entry.open_connections.saturating_sub(1);
            if entry.open_connections > 0 {
                return;
            }
            state.connections.remove(&device_id);
            state
                .connection_devices
                .retain(|_connection, owner| *owner != device_id);
            let mut stored = state.stored.clone();
            if let Some(device) = stored
                .devices
                .iter_mut()
                .find(|stored| stored.device_id == device_id)
            {
                device.last_seen_at_ms = Some(now_ms);
                // Last seen is a hint: losing one write must not disturb a live session.
                if self.inner.store.write(&stored).is_ok() {
                    state.stored = stored;
                } else {
                    crate::logging::warn(
                        "remote_device_last_seen_write_failed",
                        serde_json::json!({}),
                    );
                }
            }
            state.changed.clone()
        };
        notify(changed);
    }

    pub(crate) fn note_protocol_connection(&self, device: &PublicKey, connection_id: &str) {
        self.state()
            .connection_devices
            .insert(connection_id.to_string(), device.to_string());
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.inner
            .state
            .lock()
            .expect("remote devices lock poisoned")
    }
}

impl RemoteDevicesWorkflow for RemoteDevices {
    fn snapshot(&self) -> DeviceCollectionSnapshot {
        let state = self.state();
        DeviceCollectionSnapshot {
            remote_access: state.access,
            server_name: self.inner.server_name.clone(),
            devices: state
                .stored
                .devices
                .iter()
                .map(|device| RemoteDeviceSummary {
                    device_id: device.device_id.clone(),
                    name: device.name.clone(),
                    model: device.model.clone(),
                    added_at_ms: device.added_at_ms,
                    added_by: device.added_by.clone(),
                    last_seen_at_ms: device.last_seen_at_ms,
                    connection: state
                        .connections
                        .get(&device.device_id)
                        .map(|connected| connected.connection.clone()),
                })
                .collect(),
        }
    }

    fn create_invite(
        &self,
        added_by: String,
        now_ms: u64,
    ) -> Result<DevicesCreateInviteResult, ProtocolError> {
        let mut state = self.state();
        if state.edge.is_none() {
            return Err(remote_access_unavailable());
        }
        let mut stored = state.stored.clone();
        let secret_key = stored.ensure_secret_key();
        if stored != state.stored {
            self.inner.store.write(&stored).map_err(storage_error)?;
            state.stored = stored;
        }
        let secret: [u8; INVITE_SECRET_LEN] = SecretKey::generate().to_bytes()[..INVITE_SECRET_LEN]
            .try_into()
            .expect("an invite secret is shorter than a key");
        let expires_at_ms = now_ms.saturating_add(INVITE_LIFETIME_MS);
        state.invite = Some(Invite {
            secret,
            added_by,
            expires_at_ms,
        });
        state.send(EdgeCommand::Reconcile);
        Ok(DevicesCreateInviteResult {
            code: InviteCode {
                server: secret_key.public(),
                secret,
            }
            .encode(),
            expires_at_ms,
        })
    }

    fn cancel_invite(&self) {
        let mut state = self.state();
        if state.invite.take().is_some() {
            state.forget_identity_if_unused(&self.inner.store);
            state.send(EdgeCommand::Reconcile);
        }
    }

    fn preview_join_request(
        &self,
        code: &str,
    ) -> Result<DevicesPreviewJoinRequestResult, ProtocolError> {
        let join = JoinCode::decode(code).map_err(pairing_code_error)?;
        let device_id = join.device.to_string();
        Ok(DevicesPreviewJoinRequestResult {
            already_trusted: self
                .state()
                .stored
                .devices
                .iter()
                .any(|device| device.device_id == device_id),
            device_id,
            name: join.name,
            model: join.model,
            server_name: self.inner.server_name.clone(),
        })
    }

    fn approve_join_request(
        &self,
        code: &str,
        added_by: String,
        now_ms: u64,
    ) -> Result<(), ProtocolError> {
        let join = JoinCode::decode(code).map_err(pairing_code_error)?;
        let mut state = self.state();
        if state.edge.is_none() {
            return Err(remote_access_unavailable());
        }
        let mut stored = state.stored.clone();
        stored.ensure_secret_key();
        trust(
            &mut stored,
            &join.device,
            &join.name,
            join.model.as_deref(),
            added_by,
            now_ms,
        );
        self.inner.store.write(&stored).map_err(storage_error)?;
        state.stored = stored;
        state.send(EdgeCommand::Reconcile);
        state.send(EdgeCommand::AnnounceTrust(join.device));
        Ok(())
    }

    fn remove(&self, device_id: &str) -> Result<(), ProtocolError> {
        let mut state = self.state();
        let mut stored = state.stored.clone();
        let before = stored.devices.len();
        stored
            .devices
            .retain(|device| device.device_id != device_id);
        if stored.devices.len() == before {
            return Err(ProtocolError {
                code: ProtocolErrorCode::NotFound,
                message: "This device is no longer connected to this computer".to_string(),
                recoverable: true,
                target: None,
            });
        }
        if stored.devices.is_empty() && state.invite.is_none() {
            stored.forget_secret_key();
        }
        self.inner.store.write(&stored).map_err(storage_error)?;
        state.stored = stored;
        state.connections.remove(device_id);
        state
            .connection_devices
            .retain(|_connection, owner| owner != device_id);
        if let Ok(device) = device_id.parse::<PublicKey>() {
            state.send(EdgeCommand::Disconnect(device));
        }
        state.send(EdgeCommand::Reconcile);
        Ok(())
    }

    fn device_name_for_connection(&self, connection_id: &str) -> Option<String> {
        let state = self.state();
        let device_id = state.connection_devices.get(connection_id)?;
        state
            .stored
            .devices
            .iter()
            .find(|device| device.device_id == *device_id)
            .map(|device| device.name.clone())
    }
}

impl State {
    fn wants_endpoint(&self) -> bool {
        !self.stored.devices.is_empty() || self.invite.is_some()
    }

    fn send(&self, command: EdgeCommand) {
        if let Some(edge) = &self.edge {
            edge(command);
        }
    }

    /// An invite nobody used leaves no reason to keep an identity.
    fn forget_identity_if_unused(&mut self, store: &RemoteDeviceStore) {
        if self.wants_endpoint() || self.stored.secret_key().is_none() {
            return;
        }
        let mut stored = self.stored.clone();
        stored.forget_secret_key();
        if store.write(&stored).is_ok() {
            self.stored = stored;
        }
    }
}

/// Stands in where no network edge exists, such as the stdio protocol mode.
pub(crate) struct NoRemoteDevices;

impl RemoteDevicesWorkflow for NoRemoteDevices {
    fn snapshot(&self) -> DeviceCollectionSnapshot {
        DeviceCollectionSnapshot {
            remote_access: RemoteAccessState::Off,
            server_name: String::new(),
            devices: Vec::new(),
        }
    }

    fn create_invite(
        &self,
        _added_by: String,
        _now_ms: u64,
    ) -> Result<DevicesCreateInviteResult, ProtocolError> {
        Err(remote_access_unavailable())
    }

    fn cancel_invite(&self) {}

    fn preview_join_request(
        &self,
        _code: &str,
    ) -> Result<DevicesPreviewJoinRequestResult, ProtocolError> {
        Err(remote_access_unavailable())
    }

    fn approve_join_request(
        &self,
        _code: &str,
        _added_by: String,
        _now_ms: u64,
    ) -> Result<(), ProtocolError> {
        Err(remote_access_unavailable())
    }

    fn remove(&self, _device_id: &str) -> Result<(), ProtocolError> {
        Err(remote_access_unavailable())
    }

    fn device_name_for_connection(&self, _connection_id: &str) -> Option<String> {
        None
    }
}

/// Pairing the same key again refreshes its labels and keeps one entry.
fn trust(
    stored: &mut StoredRemoteDevices,
    device: &PublicKey,
    name: &str,
    model: Option<&str>,
    added_by: String,
    now_ms: u64,
) {
    let device_id = device.to_string();
    stored
        .devices
        .retain(|device| device.device_id != device_id);
    let name = clamp_label(name);
    stored.devices.push(StoredRemoteDevice {
        device_id,
        name: if name.is_empty() {
            "Unnamed device".to_string()
        } else {
            name
        },
        model: model.map(clamp_label).filter(|model| !model.is_empty()),
        added_at_ms: now_ms,
        added_by: Some(added_by),
        last_seen_at_ms: None,
    });
}

fn notify(changed: Option<Arc<dyn Fn() + Send + Sync>>) {
    if let Some(changed) = changed {
        changed();
    }
}

fn constant_time_eq(expected: &[u8], actual: &[u8]) -> bool {
    expected.len() == actual.len()
        && expected
            .iter()
            .zip(actual)
            .fold(0u8, |difference, (left, right)| difference | (left ^ right))
            == 0
}

fn remote_access_unavailable() -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::CapabilityUnavailable,
        message: "Remote devices are not available in this app".to_string(),
        recoverable: false,
        target: None,
    }
}

fn pairing_code_error(error: PairingCodeError) -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::ValidationFailed,
        message: error.to_string(),
        recoverable: true,
        target: None,
    }
}

fn storage_error(_error: crate::protocol::errors::RuntimeError) -> ProtocolError {
    crate::logging::error("remote_devices_store_write_failed", serde_json::json!({}));
    ProtocolError {
        code: ProtocolErrorCode::Internal,
        message: "OpenAIDE could not save the device list".to_string(),
        recoverable: true,
        target: None,
    }
}

#[cfg(test)]
#[path = "remote_devices_tests.rs"]
mod tests;
