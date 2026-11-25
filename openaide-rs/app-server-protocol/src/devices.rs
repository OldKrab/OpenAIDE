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
    /// The Own Relay this App Server is reached through (ADR-0085). Absent while it
    /// uses the public relays. Relay keys never appear here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub own_relay: Option<OwnRelaySnapshot>,
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

/// The Own Relay as clients may show it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct OwnRelaySnapshot {
    /// The relay's host name or address.
    pub host: String,
    /// Whether the App Server holds a relay key that may add and revoke people, so
    /// clients offer `devices/relayPeople` and its siblings.
    pub can_manage: bool,
    pub access: OwnRelayAccess,
}

/// What the relay last said about this App Server's relay key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum OwnRelayAccess {
    /// The relay accepts the key.
    Accepted,
    /// The relay no longer accepts the key: it was revoked. Devices outside this
    /// computer's network cannot reach it until the relay key is replaced or cleared.
    Refused,
    /// Not known yet, or the relay could not be asked.
    Unknown,
}

/// Moves this App Server onto an Own Relay, or back to the public relays when
/// `relay` is absent (ADR-0085). `relay` is a relay key: the relay's `https`
/// address with `?token=<key>`. The App Server asks the relay before it accepts a
/// key. A key that may manage the relay is kept for managing only; the App Server
/// then makes itself an ordinary key, named after this computer, to be reached with.
///
/// A change removes every paired device and cancels the pending invite, because a
/// device knows only the relay it paired through. Setting the current relay again
/// changes nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DevicesSetRelayParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub relay: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DevicesSetRelayResult {
    /// How many paired devices the change removed.
    pub removed_devices: u32,
}

/// Lists everyone holding a key to this App Server's Own Relay. Needs a relay key
/// that may manage the relay.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DevicesRelayPeopleParams {}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DevicesRelayPeopleResult {
    /// In the order the keys were made.
    pub people: Vec<RelayPerson>,
}

/// Someone holding a key to the Own Relay: a person, or a computer such as this one.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RelayPerson {
    pub name: String,
    pub can_manage: bool,
    /// The key this App Server is reached with, or the one it manages with.
    pub this_computer: bool,
}

/// Makes a new relay key for `name`. The key is returned once, to be handed to
/// that person; the App Server does not keep it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DevicesRelayAddPersonParams {
    pub name: String,
    #[serde(default)]
    pub can_manage: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DevicesRelayAddPersonResult {
    pub person: RelayPerson,
    /// The relay key line, secret included.
    pub key: String,
}

/// Revokes `name`'s relay key. Their computer and its devices lose the relay at
/// once. The last key that may manage the relay cannot be revoked.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DevicesRelayRevokePersonParams {
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DevicesRelayRevokePersonResult {}
