//! `openaide-app-server remote relay`: shows or changes the relay this computer
//! is reached through (ADR-0085).
//!
//! A relay key carries a password, so it is read from standard input rather
//! than from the arguments, which other users of the computer and the shell
//! history can see. Only the relay's host is ever printed.

use std::io::{BufRead, IsTerminal, Write};

use openaide_app_server_protocol::devices::{
    DeviceCollectionSnapshot, DevicesSetRelayResult, OwnRelayAccess,
};
use openaide_app_server_protocol::methods::DEVICES_SET_RELAY;
use openaide_remote_core::OwnRelay;
use serde_json::json;

use super::local_client::{ClientError, LocalClient};
use super::Failure;

/// Prints the relay this computer uses.
pub(super) fn show(client: &mut LocalClient) -> Result<(), Failure> {
    println!("{}", describe(&client.devices()?));
    Ok(())
}

pub(super) fn describe(snapshot: &DeviceCollectionSnapshot) -> String {
    let Some(relay) = &snapshot.own_relay else {
        return "This computer is reached through the free public relays.".to_string();
    };
    let host = &relay.host;
    match relay.access {
        OwnRelayAccess::Refused => format!(
            "The relay at {host} no longer accepts this computer. Set a new relay key, or go back to the free relays with: openaide-app-server remote relay clear"
        ),
        _ if relay.can_manage => format!(
            "This computer is reached through its own relay at {host}, and can add and remove people in Settings > Remote."
        ),
        _ => format!("This computer is reached through its own relay at {host}."),
    }
}

/// Sets the Own Relay from a relay key read on standard input, or returns to the
/// public relays when `set` is false. The App Server checks the key with the relay.
pub(super) fn change(client: &mut LocalClient, set: bool, confirmed: bool) -> Result<(), Failure> {
    let relay = if set { Some(read_line()?) } else { None };
    let snapshot = client.devices()?;
    // Only a return to the public relays already in use is skipped: the same relay
    // host may come with a new password.
    if relay.is_none() && snapshot.own_relay.is_none() {
        println!("{}", describe(&snapshot));
        return Ok(());
    }
    let paired = snapshot.devices.len();
    if paired > 0 && !confirmed {
        confirm(paired)?;
    }
    let line = relay.as_ref().map(OwnRelay::line);
    let answer = client.request(DEVICES_SET_RELAY, json!({ "relay": line }))?;
    let result: DevicesSetRelayResult =
        serde_json::from_value(answer.result).map_err(|_| ClientError::Malformed)?;
    println!("{}", describe(&client.devices()?));
    if result.removed_devices > 0 {
        println!(
            "{} paired device(s) were removed. Pair them again with: openaide-app-server remote pair",
            result.removed_devices
        );
    }
    Ok(())
}

fn read_line() -> Result<OwnRelay, Failure> {
    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        print!("Paste your relay key, then press Enter: ");
        let _ = std::io::stdout().flush();
    }
    let mut line = String::new();
    let _ = stdin.lock().read_line(&mut line);
    OwnRelay::parse(&line).map_err(Failure::BadRelay)
}

fn confirm(paired: usize) -> Result<(), Failure> {
    // A piped line has already used standard input, so nothing could answer.
    if !std::io::stdin().is_terminal() {
        return Err(Failure::RelayNeedsConfirmation(paired));
    }
    print!(
        "Changing the relay removes the {paired} paired device(s); each must be paired again. Continue? (y/n) "
    );
    let _ = std::io::stdout().flush();
    let mut answer = String::new();
    let _ = std::io::stdin().lock().read_line(&mut answer);
    if matches!(answer.trim(), "y" | "Y" | "yes") {
        Ok(())
    } else {
        Err(Failure::RelayKept)
    }
}
