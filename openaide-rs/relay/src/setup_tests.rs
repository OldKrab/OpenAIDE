use std::net::IpAddr;
use std::path::Path;

use super::*;

fn ips(list: &[&str]) -> Vec<IpAddr> {
    list.iter().map(|ip| ip.parse().expect("ip")).collect()
}

#[test]
fn a_global_ipv4_address_wins_then_a_global_ipv6_one() {
    let both = ips(&[
        "127.0.0.1",
        "::1",
        "10.0.0.5",
        "2a01:4f8::1",
        "203.0.113.9",
        "198.51.100.1",
    ]);
    // 203.0.113.0/24 and 198.51.100.0/24 are documentation ranges, so neither counts.
    assert_eq!(
        choose_public_ip(&both),
        IpChoice::Public("2a01:4f8::1".parse().expect("ip"))
    );
    let with_v4 = ips(&["10.0.0.5", "fe80::1", "2a01:4f8::1", "95.216.1.2"]);
    assert_eq!(
        choose_public_ip(&with_v4),
        IpChoice::Public("95.216.1.2".parse().expect("ip"))
    );
}

#[test]
fn private_carrier_nat_and_link_local_addresses_are_not_public() {
    let private = ips(&[
        "127.0.0.1",
        "10.1.2.3",
        "172.16.0.1",
        "192.168.1.10",
        "100.64.0.1",
        "169.254.1.1",
        "::1",
        "fe80::1",
        "fd00::1",
        "2001:db8::1",
    ]);
    assert_eq!(choose_public_ip(&private), IpChoice::OnlyPrivate);
    assert_eq!(
        choose_public_ip(&ips(&["127.0.0.1", "::1"])),
        IpChoice::None
    );
    assert_eq!(choose_public_ip(&[]), IpChoice::None);
}

fn options() -> SetupOptions {
    SetupOptions {
        name: None,
        ip: None,
        port: 443,
        cert_dir: None,
        acme_directory: None,
        state_dir: DEFAULT_STATE_DIR.into(),
    }
}

#[test]
fn the_plan_uses_the_given_address_or_the_detected_one() {
    let detected = plan(&options(), &ips(&["10.0.0.5", "95.216.1.2"])).expect("planned");
    assert_eq!(detected.host, Host::Ip("95.216.1.2".parse().expect("ip")));
    assert_eq!(detected.cert, CertSource::Acme);
    assert_eq!(detected.quic_port, Some(7842));

    let named = SetupOptions {
        name: Some("Relay.Example.com".into()),
        ..options()
    };
    assert_eq!(
        plan(&named, &[]).expect("planned").host,
        Host::Name("relay.example.com".into())
    );
    let ip = SetupOptions {
        ip: Some("2a01:4f8::1".parse().expect("ip")),
        ..options()
    };
    assert_eq!(
        plan(&ip, &[]).expect("planned").host,
        Host::Ip("2a01:4f8::1".parse().expect("ip"))
    );
}

#[test]
fn the_plan_refuses_what_cannot_work() {
    let both = SetupOptions {
        name: Some("relay.example.com".into()),
        ip: Some("95.216.1.2".parse().expect("ip")),
        ..options()
    };
    assert!(plan(&both, &[]).is_err());
    let bad_name = SetupOptions {
        name: Some("not a name".into()),
        ..options()
    };
    assert!(plan(&bad_name, &[]).is_err());

    let behind_router = plan(&options(), &ips(&["192.168.1.10"])).expect_err("refused");
    assert!(behind_router.0.contains("--ip"), "{}", behind_router.0);
    assert!(behind_router.0.contains("Forward"), "{}", behind_router.0);

    let other_port = SetupOptions {
        port: 8443,
        ..options()
    };
    let refusal = plan(&other_port, &ips(&["95.216.1.2"])).expect_err("refused");
    assert!(refusal.0.contains("--cert-dir"), "{}", refusal.0);
    let with_files = SetupOptions {
        port: 8443,
        cert_dir: Some("/etc/letsencrypt/live/relay".into()),
        ..options()
    };
    let planned = plan(&with_files, &ips(&["95.216.1.2"])).expect("planned");
    assert_eq!(planned.port, 8443);
    assert_eq!(
        planned.cert,
        CertSource::Files {
            dir: "/etc/letsencrypt/live/relay".into()
        }
    );
}

#[test]
fn the_unit_runs_the_installed_program_confined_to_its_state() {
    let unit = unit_text(Path::new(DEFAULT_STATE_DIR));
    for line in [
        "ExecStart=/usr/local/bin/openaide-relay serve\n",
        "User=openaide-relay\n",
        "AmbientCapabilities=CAP_NET_BIND_SERVICE\n",
        "ProtectSystem=strict\n",
        "ReadWritePaths=/var/lib/openaide-relay\n",
        "Restart=always\n",
        "WantedBy=multi-user.target\n",
    ] {
        assert!(unit.contains(line), "missing {line:?} in\n{unit}");
    }
    let custom = unit_text(Path::new("/srv/relay"));
    assert!(
        custom.contains("ExecStart=/usr/local/bin/openaide-relay serve --state-dir /srv/relay\n")
    );
    assert!(custom.contains("ReadWritePaths=/srv/relay\n"));
}
