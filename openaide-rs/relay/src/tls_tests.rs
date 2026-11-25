use std::sync::Arc;

use rustls_pki_types::{CertificateDer, ServerName};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::*;

/// A test CA and a certificate it issued for `names`.
pub(crate) struct TestCertificates {
    pub ca: CertificateDer<'static>,
    pub chain_pem: String,
    pub key_pem: String,
}

pub(crate) fn test_certificates(names: &[&str]) -> TestCertificates {
    let ca_key = rcgen::KeyPair::generate().expect("CA key");
    let mut ca_params = rcgen::CertificateParams::new(Vec::<String>::new()).expect("CA params");
    ca_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    ca_params
        .distinguished_name
        .push(rcgen::DnType::CommonName, "OpenAIDE relay test CA");
    let ca = rcgen::CertifiedIssuer::self_signed(ca_params, ca_key).expect("CA");
    let leaf_key = rcgen::KeyPair::generate().expect("leaf key");
    let names: Vec<String> = names.iter().map(|name| name.to_string()).collect();
    let leaf = rcgen::CertificateParams::new(names)
        .expect("leaf params")
        .signed_by(&leaf_key, &ca)
        .expect("leaf");
    TestCertificates {
        ca: ca.der().clone(),
        chain_pem: format!("{}{}", leaf.pem(), ca.pem()),
        key_pem: leaf_key.serialize_pem(),
    }
}

pub(crate) fn write_cert_dir(dir: &Path, certificates: &TestCertificates) {
    std::fs::write(dir.join("fullchain.pem"), &certificates.chain_pem).expect("chain");
    std::fs::write(dir.join("privkey.pem"), &certificates.key_pem).expect("key");
}

fn key_of(certificates: &TestCertificates) -> Arc<CertifiedKey> {
    certified_key_from_pem(
        certificates.chain_pem.as_bytes(),
        certificates.key_pem.as_bytes(),
    )
    .expect("certificate")
}

#[test]
fn only_an_acme_tls_handshake_gets_the_validation_certificate() {
    let resolver = CertResolver::new();
    assert!(resolver.choose(false).is_none());
    let current = key_of(&test_certificates(&["relay.test"]));
    resolver.set_current(current.clone());
    assert!(
        resolver.choose(true).is_none(),
        "no validation certificate without an open order"
    );
    let challenge = crate::acme::challenge_certificate(
        &crate::config::Host::parse("relay.test").expect("host"),
        &[7; 32],
    )
    .expect("validation certificate");
    resolver.set_challenge(Some(challenge.clone()));
    assert!(Arc::ptr_eq(
        &resolver.choose(true).expect("chosen"),
        &challenge
    ));
    assert!(Arc::ptr_eq(
        &resolver.choose(false).expect("chosen"),
        &current
    ));

    let offered = |protocols: &[&'static [u8]]| offers_acme_alpn(Some(protocols.iter().copied()));
    assert!(offered(&[ACME_TLS_ALPN]));
    assert!(offered(&[b"h2", ACME_TLS_ALPN]));
    assert!(!offered(&[b"h2", HTTP_1_1_ALPN]));
    assert!(!offers_acme_alpn(None::<std::iter::Empty<&[u8]>>));
}

/// Accepts any certificate and signature. A validation certificate carries a critical
/// extension that webpki refuses even for signature checks; an ACME validator does not
/// use webpki.
#[derive(Debug)]
struct AcceptAll;

impl rustls::client::danger::ServerCertVerifier for AcceptAll {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls_pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Runs one handshake against the relay's server config and returns the certificate the
/// client saw and the protocol it agreed on.
async fn handshake(
    resolver: Arc<CertResolver>,
    alpn: &[u8],
) -> (CertificateDer<'static>, Option<Vec<u8>>) {
    let acceptor =
        tokio_rustls::TlsAcceptor::from(Arc::new(server_config(resolver).expect("server")));
    let mut client = rustls::ClientConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .expect("versions")
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AcceptAll))
        .with_no_client_auth();
    client.alpn_protocols = vec![alpn.to_vec()];
    let (client_io, server_io) = tokio::io::duplex(64 * 1024);
    let server = tokio::spawn(async move {
        let mut stream = acceptor.accept(server_io).await.expect("server handshake");
        stream.write_all(b"x").await.expect("write");
        stream.flush().await.expect("flush");
    });
    let mut stream = tokio_rustls::TlsConnector::from(Arc::new(client))
        .connect(ServerName::try_from("relay.test").expect("name"), client_io)
        .await
        .expect("client handshake");
    let mut byte = [0u8; 1];
    stream.read_exact(&mut byte).await.expect("read");
    server.await.expect("server finished");
    let (_, connection) = stream.get_ref();
    let certificate = connection.peer_certificates().expect("certificates")[0].clone();
    (certificate, connection.alpn_protocol().map(<[u8]>::to_vec))
}

#[tokio::test]
async fn a_handshake_offering_acme_tls_is_answered_with_the_validation_certificate() {
    let resolver = CertResolver::new();
    let current = key_of(&test_certificates(&["relay.test"]));
    resolver.set_current(current.clone());
    let challenge = crate::acme::challenge_certificate(
        &crate::config::Host::parse("relay.test").expect("host"),
        &[7; 32],
    )
    .expect("validation certificate");
    resolver.set_challenge(Some(challenge.clone()));

    let (seen, protocol) = handshake(resolver.clone(), ACME_TLS_ALPN).await;
    assert_eq!(seen, challenge.cert[0]);
    assert_eq!(protocol.as_deref(), Some(ACME_TLS_ALPN));
    let (seen, protocol) = handshake(resolver, HTTP_1_1_ALPN).await;
    assert_eq!(seen, current.cert[0]);
    assert_eq!(protocol.as_deref(), Some(HTTP_1_1_ALPN));
}

#[test]
fn pem_files_load_and_bad_ones_are_classified() {
    let first = test_certificates(&["relay.test"]);
    let second = test_certificates(&["relay.test"]);
    let key = key_of(&first);
    assert_eq!(key.cert.len(), 2, "leaf and CA");
    let (not_before, not_after) = validity(&key.cert[0]).expect("validity");
    assert!(not_before < not_after);

    let mismatched = certified_key_from_pem(first.chain_pem.as_bytes(), second.key_pem.as_bytes());
    assert!(matches!(mismatched, Err(CertError::KeyMismatch)));
    assert!(matches!(
        certified_key_from_pem(b"nothing", first.key_pem.as_bytes()),
        Err(CertError::NoCertificate)
    ));
    assert!(matches!(
        certified_key_from_pem(first.chain_pem.as_bytes(), b"nothing"),
        Err(CertError::BadKey)
    ));
}

#[test]
fn certbot_file_names_come_first_then_iroh_ones() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let certificates = test_certificates(&["relay.test"]);
    std::fs::write(dir.path().join("default.crt"), &certificates.chain_pem).expect("write");
    std::fs::write(dir.path().join("default.key"), &certificates.key_pem).expect("write");
    assert_eq!(cert_files(dir.path()).0, dir.path().join("default.crt"));
    load_cert_dir(dir.path()).expect("iroh names load");

    write_cert_dir(dir.path(), &certificates);
    assert_eq!(cert_files(dir.path()).0, dir.path().join("fullchain.pem"));
    assert_eq!(cert_files(dir.path()).1, dir.path().join("privkey.pem"));
}

#[test]
fn the_watcher_picks_up_renewed_files_and_keeps_the_old_certificate_on_failure() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let resolver = CertResolver::new();
    let mut watcher = CertDirWatcher::new(dir.path().to_path_buf(), resolver.clone());
    watcher.check();
    assert!(resolver.current().is_none(), "no files yet");

    let first = test_certificates(&["relay.test"]);
    write_cert_dir(dir.path(), &first);
    watcher.check();
    let loaded = resolver.current().expect("loaded");
    assert_eq!(loaded.cert, key_of(&first).cert);

    let renewed = test_certificates(&["relay.test"]);
    write_cert_dir(dir.path(), &renewed);
    watcher.check();
    assert_eq!(
        resolver.current().expect("loaded").cert,
        key_of(&renewed).cert
    );

    std::fs::write(dir.path().join("privkey.pem"), "damaged").expect("write");
    watcher.check();
    assert_eq!(
        resolver.current().expect("kept").cert,
        key_of(&renewed).cert,
        "a failed load keeps the previous certificate"
    );
}
