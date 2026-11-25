use std::net::IpAddr;
use std::time::{Duration, SystemTime};

use x509_parser::extensions::{GeneralName, ParsedExtension};

use super::*;
use crate::tls::tests::test_certificates;

// timing: data — certificate lifetimes are compared as values, never waited on.
const HOUR: Duration = Duration::from_secs(60 * 60);

fn name() -> Host {
    Host::parse("relay.example.com").expect("name")
}

fn ip() -> Host {
    Host::parse("203.0.113.7").expect("ip")
}

#[test]
fn a_name_gets_the_default_profile_and_an_ip_the_short_lived_one() {
    assert_eq!(
        identifier(&name()),
        Identifier::Dns("relay.example.com".into())
    );
    assert_eq!(
        identifier(&ip()),
        Identifier::Ip("203.0.113.7".parse::<IpAddr>().expect("ip"))
    );
    assert_eq!(profile(&name()), None);
    assert_eq!(profile(&ip()), Some("shortlived"));
}

#[test]
fn renewal_starts_when_a_third_of_the_lifetime_is_left() {
    // timing: data — a fixed issuance time, compared and never waited on.
    let start = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000);
    // A 90-day certificate renews after 60 days.
    assert_eq!(
        renew_at(start, start + 90 * 24 * HOUR),
        start + 60 * 24 * HOUR
    );
    // A 6-day (160-hour) short-lived one with about 53 hours left.
    let renew = renew_at(start, start + 160 * HOUR);
    assert_eq!(
        renew.duration_since(start).expect("after start").as_secs(),
        384_000
    );
    // A certificate whose dates are reversed renews at once.
    assert_eq!(renew_at(start + HOUR, start), start);
}

#[test]
fn retries_slow_down_to_an_hour() {
    assert_eq!(retry_pause(1), FIRST_RETRY_PAUSE);
    assert_eq!(retry_pause(2), FIRST_RETRY_PAUSE * 2);
    assert_eq!(retry_pause(3), FIRST_RETRY_PAUSE * 4);
    assert_eq!(retry_pause(7), MAX_RETRY_PAUSE);
    assert_eq!(retry_pause(u32::MAX), MAX_RETRY_PAUSE);
}

fn parsed_names_and_identifier(host: &Host, digest: &[u8]) -> (Vec<String>, Vec<u8>, bool) {
    let key = challenge_certificate(host, digest).expect("validation certificate");
    let (_, cert) = x509_parser::parse_x509_certificate(key.cert[0].as_ref()).expect("parsed");
    let mut names = Vec::new();
    let mut identifier = None;
    for extension in cert.extensions() {
        if let ParsedExtension::SubjectAlternativeName(san) = extension.parsed_extension() {
            for name in &san.general_names {
                names.push(match name {
                    GeneralName::DNSName(dns) => format!("dns:{dns}"),
                    GeneralName::IPAddress(bytes) => format!("ip:{bytes:?}"),
                    other => format!("other:{other:?}"),
                });
            }
        }
        // id-pe-acmeIdentifier, RFC 8737
        if extension.oid.to_id_string() == "1.3.6.1.5.5.7.1.31" {
            identifier = Some((extension.value.to_vec(), extension.critical));
        }
    }
    let (value, critical) = identifier.expect("acmeIdentifier extension");
    (names, value, critical)
}

#[test]
fn the_validation_certificate_names_the_host_and_carries_the_digest() {
    let digest = [0xab; 32];
    // The extension value is an OCTET STRING (tag 4, length 32) holding the digest.
    let mut expected = vec![4, 32];
    expected.extend_from_slice(&digest);

    let (names, value, critical) = parsed_names_and_identifier(&name(), &digest);
    assert_eq!(names, ["dns:relay.example.com"]);
    assert_eq!(value, expected);
    assert!(critical);

    let (names, value, _) = parsed_names_and_identifier(&ip(), &digest);
    assert_eq!(names, [format!("ip:{:?}", [203u8, 0, 113, 7])]);
    assert_eq!(value, expected);
}

#[test]
fn a_stored_certificate_is_used_only_for_its_own_host() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let certificates = test_certificates(&["relay.example.com"]);
    let stored = StoredCertificate {
        host: name(),
        chain_pem: certificates.chain_pem,
        key_pem: certificates.key_pem,
    };
    write_acme_file(
        dir.path(),
        CERTIFICATE_FILE,
        &serde_json::to_vec(&stored).expect("encoded"),
    )
    .expect("stored");
    let settings = |host| AcmeSettings {
        host,
        directory: LETS_ENCRYPT_PRODUCTION.to_string(),
        state_dir: dir.path().to_path_buf(),
    };

    let (key, renew) = load_certificate(&settings(name())).expect("loaded");
    let (not_before, not_after) = tls::validity(&key.cert[0]).expect("validity");
    assert_eq!(renew, renew_at(not_before, not_after));
    assert!(load_certificate(&settings(ip())).is_none());
}
