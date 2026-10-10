//! Remote Devices: App Shells on other machines trusted by key pair (ADR-0062).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The Remote Devices an App Server trusts, with the transient state of each connection.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCollectionSnapshot {
    pub remote_access: RemoteAccessState,
    /// The user-facing name of the machine this App Server runs on.
    pub server_name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub devices: Vec<RemoteDeviceSummary>,
}

/// Whether the App Server is reachable by Remote Devices. It is `off` until the
/// first Pairing Code and after the last Remote Device is removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum RemoteAccessState {
    Off,
    Starting,
    On,
    /// The network endpoint could not start. Pairing is unavailable until it does.
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDeviceSummary {
    /// The device's public key in text form.
    pub device_id: String,
    /// Self-reported label. It identifies the device to the user and proves nothing.
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub added_at_ms: u64,
    /// The label of the client that paired this device.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seen_at_ms: Option<u64>,
    /// Present while the device is connected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<RemoteDeviceConnection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDeviceConnection {
    pub path: RemoteDevicePath,
    /// The device's network address on a direct path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum RemoteDevicePath {
    Direct,
    Relayed,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DevicesCreateInviteParams {}

/// A single-use Pairing Code a new device reads to join this App Server. Creating
/// another invite, cancelling, or reaching `expires_at_ms` invalidates it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DevicesCreateInviteResult {
    pub code: String,
    pub expires_at_ms: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DevicesCancelInviteParams {}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DevicesCancelInviteResult {}

/// Reads the Pairing Code a new device shows, without trusting it yet.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DevicesPreviewJoinRequestParams {
    pub code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DevicesPreviewJoinRequestResult {
    pub device_id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// The App Server the device would join, for the confirmation shown to the user.
    pub server_name: String,
    pub already_trusted: bool,
}

/// Trusts the device named by a join-request Pairing Code the user confirmed.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DevicesApproveJoinRequestParams {
    pub code: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DevicesApproveJoinRequestResult {}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DevicesRemoveParams {
    pub device_id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DevicesRemoveResult {}
