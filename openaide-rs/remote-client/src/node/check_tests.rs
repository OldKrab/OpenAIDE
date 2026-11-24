use super::*;

const WIFI: NetworkKind = NetworkKind {
    transport: Transport::Wifi,
    validated: true,
    vpn: false,
};

fn result(network: NetworkKind, relay: CheckRelay, computer: CheckComputer) -> CheckResult {
    CheckResult {
        network,
        relay,
        computer,
    }
}

#[test]
fn each_failing_step_is_named_in_the_record() {
    let offline = result(
        NetworkKind::NONE,
        CheckRelay::NotReached,
        CheckComputer::Unreachable,
    );
    assert_eq!(offline.outcome(), "offline");
    let no_endpoint = result(WIFI, CheckRelay::NoEndpoint, CheckComputer::NotChecked);
    assert_eq!(no_endpoint.outcome(), "no_endpoint");
    let blocked = result(WIFI, CheckRelay::NotReached, CheckComputer::Unreachable);
    assert_eq!(blocked.outcome(), "relay_not_reached");
    let silent = result(WIFI, CheckRelay::Reached, CheckComputer::Unreachable);
    assert_eq!(silent.outcome(), "unreachable");
    let removed = result(WIFI, CheckRelay::Reached, CheckComputer::NotTrusted);
    assert_eq!(removed.outcome(), "not_trusted");
}

#[test]
fn a_computer_reached_without_the_relay_is_still_reached() {
    let local = result(WIFI, CheckRelay::NotReached, CheckComputer::Direct);
    assert_eq!(local.outcome(), "ok");
}

#[test]
fn the_record_carries_every_step() {
    assert_eq!(
        result(WIFI, CheckRelay::Reached, CheckComputer::Relay).fields(),
        serde_json::json!({
            "outcome": "ok", "network": "wifi", "network_validated": true, "vpn": false,
            "relay": "reached", "computer": "relay",
        })
    );
}
