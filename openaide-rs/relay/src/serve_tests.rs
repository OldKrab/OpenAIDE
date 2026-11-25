//! The relay end to end: real HTTPS on a loopback port with a certificate from a test CA,
//! iroh endpoints and relay clients, and the management API over HTTPS.

use std::net::Ipv4Addr;
use std::time::Duration;

use iroh::endpoint::presets;
use iroh::{Endpoint, EndpointAddr, RelayMap, RelayMode, RelayUrl, SecretKey};
use iroh_relay::client::{Client, ClientBuilder, ConnectError};
use iroh_relay::tls::CaTlsConfig;
use n0_future::StreamExt;
use openaide_remote_core::OwnRelay;
use rustls_pki_types::CertificateDer;

use super::*;
use crate::keys::Person;
use crate::probe;
use crate::tls::tests::{test_certificates, write_cert_dir};

/// The suite's watchdog: bounds a hang while waiting for an outcome the test made certain.
const WATCHDOG: Duration = Duration::from_secs(30);
const TEST_ALPN: &[u8] = b"openaide-relay/test";

struct TestRelay {
    _dir: tempfile::TempDir,
    relay: RunningRelay,
    url: RelayUrl,
    ca: CertificateDer<'static>,
    owner: Person,
    member: Person,
}

impl TestRelay {
    async fn start() -> Self {
        let dir = tempfile::tempdir().expect("temporary directory");
        let certs = dir.path().join("certs");
        let state = dir.path().join("state");
        std::fs::create_dir_all(&certs).expect("certificate folder");
        std::fs::create_dir_all(&state).expect("state folder");
        let certificates = test_certificates(&["127.0.0.1"]);
        write_cert_dir(&certs, &certificates);
        let store = KeyStore::open(&state);
        let owner = store.add("owner", true).expect("added");
        let member = store.add("Pat", false).expect("added");

        let relay = RunningRelay::start(ServeOptions {
            state_dir: state,
            https_bind: (Ipv4Addr::LOCALHOST, 0).into(),
            quic_bind: Some((Ipv4Addr::LOCALHOST, 0).into()),
            host: Host::Ip(Ipv4Addr::LOCALHOST.into()),
            public_port: None,
            cert: CertSource::Files { dir: certs },
            acme_directory: acme::LETS_ENCRYPT_PRODUCTION.to_string(),
        })
        .await
        .expect("relay started");
        assert!(relay.has_certificate());
        let url = format!("https://127.0.0.1:{}/", relay.https_addr().port())
            .parse()
            .expect("relay URL");
        Self {
            _dir: dir,
            relay,
            url,
            ca: certificates.ca,
            owner,
            member,
        }
    }

    fn ca_config(&self) -> CaTlsConfig {
        CaTlsConfig::custom_roots([self.ca.clone()])
    }

    /// A relay connection as iroh's endpoints make it, with `token` as the relay key.
    async fn relay_client(&self, token: &str) -> Result<Client, ConnectError> {
        let tls = self
            .ca_config()
            .client_config(tls::provider())
            .expect("client TLS");
        ClientBuilder::new(
            self.url.clone(),
            SecretKey::generate(),
            iroh::dns::DnsResolver::new(),
        )
        .tls_client_config(tls)
        .auth_token(token)
        .connect()
        .await
    }

    /// An endpoint that reaches others only through this relay.
    async fn endpoint(&self, token: &str) -> Endpoint {
        let endpoint = Endpoint::builder(presets::Minimal)
            .relay_mode(RelayMode::Custom(
                RelayMap::from(self.url.clone()).with_auth_token(token),
            ))
            .ca_tls_config(self.ca_config())
            .clear_ip_transports()
            .alpns(vec![TEST_ALPN.to_vec()])
            .bind()
            .await
            .expect("endpoint");
        tokio::time::timeout(WATCHDOG, endpoint.online())
            .await
            .expect("the endpoint reached its relay");
        endpoint
    }

    fn https_client(&self) -> reqwest::Client {
        let mut roots = rustls::RootCertStore::empty();
        roots.add(self.ca.clone()).expect("test CA");
        let tls = rustls::ClientConfig::builder_with_provider(tls::provider())
            .with_safe_default_protocol_versions()
            .expect("versions")
            .with_root_certificates(roots)
            .with_no_client_auth();
        reqwest::Client::builder()
            .use_preconfigured_tls(tls)
            .build()
            .expect("HTTPS client")
    }

    async fn api(
        &self,
        method: reqwest::Method,
        path_and_query: &str,
        token: &str,
    ) -> (u16, serde_json::Value) {
        let response = self
            .https_client()
            .request(
                method,
                format!(
                    "{}{}",
                    self.url.as_str().trim_end_matches('/'),
                    path_and_query
                ),
            )
            .bearer_auth(token)
            .send()
            .await
            .expect("answered");
        let status = response.status().as_u16();
        let body = response.text().await.expect("body");
        (status, serde_json::from_str(&body).expect("JSON body"))
    }
}

/// Waits until `client`'s connection ends, which only the relay can cause here.
async fn closed(mut client: Client) {
    tokio::time::timeout(WATCHDOG, async {
        while let Some(Ok(_)) = client.next().await {}
    })
    .await
    .expect("the relay closed the connection");
}

#[tokio::test(flavor = "multi_thread")]
async fn two_endpoints_with_keys_reach_each_other_through_the_relay() {
    let relay = TestRelay::start().await;
    assert!(
        relay.relay.quic_addr().is_some(),
        "QUIC address discovery runs"
    );
    let server = relay.endpoint(&relay.owner.token).await;
    let client = relay.endpoint(&relay.member.token).await;
    assert_eq!(relay.relay.access().live_connections(&relay.owner.token), 1);
    assert_eq!(
        relay.relay.access().live_connections(&relay.member.token),
        1
    );

    let server_id = server.id();
    let echo = tokio::spawn(async move {
        let connection = server
            .accept()
            .await
            .expect("incoming")
            .await
            .expect("accepted");
        let (mut send, mut receive) = connection.accept_bi().await.expect("stream");
        let message = receive.read_to_end(1024).await.expect("read");
        send.write_all(&message).await.expect("write");
        send.finish().expect("finish");
        connection.closed().await;
    });
    // The only address given is the relay, and neither endpoint has an IP transport.
    let address = EndpointAddr::new(server_id).with_relay_url(relay.url.clone());
    let reply = tokio::time::timeout(WATCHDOG, async {
        let connection = client.connect(address, TEST_ALPN).await.expect("connected");
        let (mut send, mut receive) = connection.open_bi().await.expect("stream");
        send.write_all(b"hello through the relay")
            .await
            .expect("write");
        send.finish().expect("finish");
        let reply = receive.read_to_end(1024).await.expect("reply");
        connection.close(0u32.into(), b"done");
        reply
    })
    .await
    .expect("echoed");
    assert_eq!(reply, b"hello through the relay");
    tokio::time::timeout(WATCHDOG, echo)
        .await
        .expect("echo finished")
        .expect("echo task");
    relay.relay.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_wrong_or_missing_key_is_refused() {
    let relay = TestRelay::start().await;
    let admitted = relay
        .relay_client(&relay.member.token)
        .await
        .expect("a valid key connects");
    for token in ["0".repeat(48), String::new()] {
        let refused = relay.relay_client(&token).await.expect_err("refused");
        assert!(refused.to_string().contains("denied"), "{refused}");
    }
    assert_eq!(relay.relay.access().live_connections(&"0".repeat(48)), 0);
    drop(admitted);
}

#[tokio::test(flavor = "multi_thread")]
async fn deleting_a_key_cuts_its_connection_and_refuses_it_again() {
    let relay = TestRelay::start().await;
    let member = relay
        .relay_client(&relay.member.token)
        .await
        .expect("connected");
    let owner = relay
        .relay_client(&relay.owner.token)
        .await
        .expect("connected");

    let (status, body) = relay
        .api(
            reqwest::Method::DELETE,
            "/openaide/relay/v1/people?name=Pat",
            &relay.owner.token,
        )
        .await;
    assert_eq!((status, body), (200, serde_json::json!({})));
    closed(member).await;
    let refused = relay
        .relay_client(&relay.member.token)
        .await
        .expect_err("a revoked key is refused");
    assert!(refused.to_string().contains("denied"), "{refused}");
    assert_eq!(
        relay.relay.access().live_connections(&relay.member.token),
        0
    );
    assert_eq!(
        relay.relay.access().live_connections(&relay.owner.token),
        1,
        "other keys keep their connections"
    );
    drop(owner);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_key_revoked_by_another_process_is_cut_without_a_restart() {
    let relay = TestRelay::start().await;
    let member = relay
        .relay_client(&relay.member.token)
        .await
        .expect("connected");
    // The operator's command writes the key file through its own store, as another
    // process does; nothing tells the running relay.
    KeyStore::open(&relay._dir.path().join("state"))
        .revoke("Pat")
        .expect("revoked");
    closed(member).await;
    assert!(relay.relay_client(&relay.member.token).await.is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn the_management_api_answers_over_https() {
    let relay = TestRelay::start().await;
    let (status, me) = relay
        .api(
            reqwest::Method::GET,
            "/openaide/relay/v1/me",
            &relay.member.token,
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(me, serde_json::json!({ "name": "Pat", "canManage": false }));

    let (status, refused) = relay
        .api(
            reqwest::Method::GET,
            "/openaide/relay/v1/people",
            &relay.member.token,
        )
        .await;
    assert_eq!(
        (status, refused),
        (403, serde_json::json!({ "error": "cannot_manage" }))
    );

    let (status, added) = relay
        .api(
            reqwest::Method::POST,
            "/openaide/relay/v1/people?name=Olga&canManage=false",
            &relay.owner.token,
        )
        .await;
    assert_eq!(status, 200);
    let line = added["key"].as_str().expect("key line");
    let parsed = OwnRelay::parse(line).expect("OpenAIDE reads the line");
    assert_eq!(parsed.url(), &relay.url);
    let olga = relay
        .relay_client(line.rsplit_once("token=").expect("token").1)
        .await
        .expect("the new key works at once");
    drop(olga);

    let (status, people) = relay
        .api(
            reqwest::Method::GET,
            "/openaide/relay/v1/people",
            &relay.owner.token,
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(people["people"].as_array().expect("list").len(), 3);

    let client = relay.https_client();
    let ping = client
        .get(format!("{}ping", relay.url))
        .send()
        .await
        .expect("answered");
    assert_eq!(ping.status().as_u16(), 200);
    let index = client
        .get(relay.url.to_string())
        .send()
        .await
        .expect("answered")
        .text()
        .await
        .expect("text");
    assert!(index.contains("OpenAIDE relay"), "{index}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_setup_probe_checks_the_certificate() {
    let relay = TestRelay::start().await;
    let host = Host::Ip(Ipv4Addr::LOCALHOST.into());
    let port = relay.relay.https_addr().port();
    assert_eq!(probe::ping(&host, port, false).await, Ok(()));
    assert_eq!(
        probe::ping(&host, port, true).await,
        Err(probe::ProbeFailure::NoValidCertificate),
        "a test CA is not publicly trusted"
    );
}
