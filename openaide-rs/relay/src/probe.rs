//! Asks a relay on this machine whether it answers `GET /ping` over HTTPS, as `setup` does
//! before it hands out the first key.
//!
//! The probe dials the local port but checks the certificate against the public host, so
//! it works on a machine that cannot reach its own public address (a home router without
//! hairpin NAT) while still proving the certificate is valid for that address.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use iroh_relay::tls::CaTlsConfig;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::config::Host;
use crate::tls;

/// How long one probe may take.
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// Why a probe failed, worded for the operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeFailure {
    /// Nothing accepts connections on the port yet.
    NotListening,
    /// The relay answers but has no valid certificate for the host yet.
    NoValidCertificate,
    /// The relay answered something other than 200.
    BadAnswer,
}

impl ProbeFailure {
    pub fn describe(self) -> &'static str {
        match self {
            Self::NotListening => "the relay is not answering yet",
            Self::NoValidCertificate => "the relay has no valid certificate yet",
            Self::BadAnswer => "the relay answered, but not as expected",
        }
    }
}

/// Probes `https://<host>:<port>/ping` through this machine's loopback address.
/// `verify` false accepts any certificate, for a test ACME directory whose root is not
/// publicly trusted.
pub async fn ping(host: &Host, port: u16, verify: bool) -> Result<(), ProbeFailure> {
    tokio::time::timeout(PROBE_TIMEOUT, ping_once(host, port, verify))
        .await
        .unwrap_or(Err(ProbeFailure::NotListening))
}

async fn ping_once(host: &Host, port: u16, verify: bool) -> Result<(), ProbeFailure> {
    let provider = tls::provider();
    let ca = if verify {
        CaTlsConfig::embedded()
    } else {
        CaTlsConfig::custom_server_cert_verifier(Arc::new(|provider| {
            Ok(Arc::new(AnyCertificate(provider)) as Arc<dyn ServerCertVerifier>)
        }))
    };
    let config = ca
        .client_config(provider)
        .map_err(|_| ProbeFailure::BadAnswer)?;
    let server_name = match host {
        Host::Name(name) => ServerName::try_from(name.clone()),
        Host::Ip(ip) => Ok(ServerName::IpAddress((*ip).into())),
    }
    .map_err(|_| ProbeFailure::BadAnswer)?;

    let stream = TcpStream::connect(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
        .await
        .map_err(|_| ProbeFailure::NotListening)?;
    let mut stream = tokio_rustls::TlsConnector::from(Arc::new(config))
        .connect(server_name, stream)
        .await
        .map_err(|_| ProbeFailure::NoValidCertificate)?;
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        iroh_relay::http::RELAY_PROBE_PATH,
        host.url_host()
    );
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|_| ProbeFailure::BadAnswer)?;
    let mut answer = Vec::new();
    let _ = stream.read_to_end(&mut answer).await;
    if answer.starts_with(b"HTTP/1.1 200") {
        Ok(())
    } else {
        Err(ProbeFailure::BadAnswer)
    }
}

/// Accepts any certificate but still checks the handshake signatures.
#[derive(Debug)]
struct AnyCertificate(Arc<CryptoProvider>);

impl ServerCertVerifier for AnyCertificate {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}
