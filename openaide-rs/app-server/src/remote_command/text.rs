//! What the `remote` command prints. Kept free of I/O so each line is testable.

use openaide_app_server_protocol::devices::{DeviceCollectionSnapshot, RemoteDeviceSummary};

/// A change between two device lists, in the words shown to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DeviceChange {
    Paired(String),
    Removed(String),
    Connected(String),
    Left(String),
}

impl DeviceChange {
    pub(crate) fn line(&self) -> String {
        match self {
            Self::Paired(name) => format!("{name} was paired"),
            Self::Removed(name) => format!("{name} was removed"),
            Self::Connected(name) => format!("{name} connected"),
            Self::Left(name) => format!("{name} left"),
        }
    }

    /// Stable, content-free name for diagnostics.
    pub(crate) fn class(&self) -> &'static str {
        match self {
            Self::Paired(_) => "paired",
            Self::Removed(_) => "removed",
            Self::Connected(_) => "connected",
            Self::Left(_) => "left",
        }
    }
}

/// Devices in the order every listing uses: oldest pairing first, so a number
/// keeps naming the same device while newer ones are added after it.
pub(crate) fn ordered(snapshot: &DeviceCollectionSnapshot) -> Vec<&RemoteDeviceSummary> {
    let mut devices: Vec<_> = snapshot.devices.iter().collect();
    devices.sort_by(|a, b| (a.added_at_ms, &a.device_id).cmp(&(b.added_at_ms, &b.device_id)));
    devices
}

pub(crate) fn changes(
    before: &DeviceCollectionSnapshot,
    after: &DeviceCollectionSnapshot,
) -> Vec<DeviceChange> {
    let mut found = Vec::new();
    for device in ordered(after) {
        match before
            .devices
            .iter()
            .find(|known| known.device_id == device.device_id)
        {
            None => {
                found.push(DeviceChange::Paired(label(device)));
                if device.connection.is_some() {
                    found.push(DeviceChange::Connected(label(device)));
                }
            }
            Some(known) if known.connection.is_none() && device.connection.is_some() => {
                found.push(DeviceChange::Connected(label(device)));
            }
            Some(known) if known.connection.is_some() && device.connection.is_none() => {
                found.push(DeviceChange::Left(label(device)));
            }
            Some(_) => {}
        }
    }
    for device in ordered(before) {
        if !after
            .devices
            .iter()
            .any(|known| known.device_id == device.device_id)
        {
            found.push(DeviceChange::Removed(label(device)));
        }
    }
    found
}

pub(crate) fn label(device: &RemoteDeviceSummary) -> String {
    let name = printable(&device.name);
    let name = if name.is_empty() {
        "Device".to_string()
    } else {
        name
    };
    match device.model.as_deref().map(printable) {
        Some(model) if !model.is_empty() && model != name => format!("{name} ({model})"),
        _ => name,
    }
}

/// A device names itself, so its label is stripped of anything that could
/// move the cursor or recolor the terminal.
fn printable(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .to_string()
}

pub(crate) fn device_list(snapshot: &DeviceCollectionSnapshot) -> String {
    let devices = ordered(snapshot);
    if devices.is_empty() {
        return "No devices are paired with this computer.\n".to_string();
    }
    let mut text = String::new();
    for (index, device) in devices.iter().enumerate() {
        let state = if device.connection.is_some() {
            "connected"
        } else {
            "not connected"
        };
        text.push_str(&format!(
            "{:>3}  {}  ·  {state}\n",
            index + 1,
            label(device)
        ));
    }
    text
}

pub(crate) fn minutes_left(expires_at_ms: u64, now_ms: u64) -> u64 {
    expires_at_ms.saturating_sub(now_ms).div_ceil(60_000)
}

pub(crate) fn stop_warning(running_tasks: u64) -> String {
    let tasks = if running_tasks == 1 {
        "1 task is".to_string()
    } else {
        format!("{running_tasks} tasks are")
    };
    format!(
        "{tasks} running. Unless an OpenAIDE app is open on this computer, stopping interrupts them.\nPress Ctrl+C again to stop."
    )
}

#[cfg(test)]
#[path = "text_tests.rs"]
mod tests;
