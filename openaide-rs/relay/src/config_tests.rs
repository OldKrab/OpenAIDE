use std::net::{IpAddr, Ipv4Addr};

use openaide_remote_core::OwnRelay;

use super::*;

#[test]
fn a_key_line_leaves_out_port_443_and_brackets_ipv6() {
    let name = Host::parse("relay.example.com").expect("name");
    assert_eq!(
        key_line(&name, 443, "abc"),
        "https://relay.example.com/?token=abc"
    );
    assert_eq!(
        key_line(&name, 8443, "abc"),
        "https://relay.example.com:8443/?token=abc"
    );
    let v4 = Host::Ip(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 7)));
    assert_eq!(key_line(&v4, 443, "abc"), "https://203.0.113.7/?token=abc");
    let v6 = Host::parse("2001:db8::7").expect("ip");
    assert_eq!(
        key_line(&v6, 8443, "abc"),
        "https://[2001:db8::7]:8443/?token=abc"
    );
}

#[test]
fn openaide_reads_every_kind_of_key_line() {
    let token = "0123456789abcdef0123456789abcdef0123456789abcdef";
    for (host, port, expected_host) in [
        ("relay.example.com", 443, "relay.example.com"),
        ("relay.example.com", 8443, "relay.example.com"),
        ("203.0.113.7", 443, "203.0.113.7"),
        ("2001:db8::7", 443, "[2001:db8::7]"),
        ("[2001:db8::7]", 7443, "[2001:db8::7]"),
    ] {
        let host = Host::parse(host).expect("host");
        let line = key_line(&host, port, token);
        let relay = OwnRelay::parse(&line).expect("OpenAIDE accepts the line");
        assert_eq!(relay.host(), expected_host);
        assert_eq!(relay.line(), line, "the line survives a round trip");
        assert_eq!(relay.url().port_or_known_default(), Some(port));
    }
}

#[test]
fn a_host_is_an_ip_address_or_a_dns_name() {
    assert_eq!(
        Host::parse(" Relay.Example.COM. ").expect("name"),
        Host::Name("relay.example.com".into())
    );
    assert_eq!(
        Host::parse("[::1]").expect("ip"),
        Host::Ip("::1".parse().expect("ip"))
    );
    for invalid in [
        "",
        "-relay.example.com",
        "relay..example.com",
        "1.2.3.4.5",
        "a b.com",
        "relay_x.com",
    ] {
        assert_eq!(Host::parse(invalid), Err(InvalidHost), "{invalid:?}");
    }
    assert_eq!(Host::parse_name("10.0.0.1"), Err(InvalidHost));
}

#[test]
fn the_config_file_round_trips() {
    let dir = tempfile::tempdir().expect("temporary directory");
    assert_eq!(RelayConfig::load(dir.path()).expect("read"), None);
    let config = RelayConfig {
        host: Host::parse("2001:db8::7").expect("ip"),
        port: 8443,
        cert: CertSource::Files {
            dir: "/etc/letsencrypt/live/relay".into(),
        },
        quic_port: Some(DEFAULT_QUIC_PORT),
        acme_directory: None,
    };
    config.save(dir.path()).expect("saved");
    assert_eq!(RelayConfig::load(dir.path()).expect("read"), Some(config));
    let text = std::fs::read_to_string(RelayConfig::path(dir.path())).expect("read");
    assert!(text.contains("\"quicPort\": 7842"), "{text}");
    assert!(text.contains("\"mode\": \"files\""), "{text}");
}
