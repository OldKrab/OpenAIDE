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
    /// The public key Remote Devices reach this App Server by, in text form. A device
    /// compares it with the key in a Pairing Code to recognize this computer's own code
    /// (ADR-0081). Absent until the first Pairing Code and after the last Remote Device
    /// is removed, because the key is made then and forgotten then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_key: Option<String>,
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

/// A single-use Pairing Code that lets one more device use this App Server. Creating
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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DevicesRemoveParams {
    pub device_id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DevicesRemoveResult {}
