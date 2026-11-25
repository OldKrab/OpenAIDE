//! The App Server's client for the management API of an OpenAIDE relay (ADR-0085).
//!
//! `openaide-relay` admits each person by their own relay key. A key that can
//! manage may also list, add and revoke keys at `/openaide/relay/v1`, with the
//! key's password as a bearer token. A relay without that API, such as a stock
//! iroh-relay with a shared password, answers `404` to `/me`.
//!
//! Calls block the calling thread for at most [`CALL_TIMEOUT`]. Callers run them
//! outside the protocol lock and outside the Remote Devices state lock.

use std::time::{Duration, Instant};

use openaide_remote_core::OwnRelay;
use serde::Deserialize;
use serde_json::json;

use crate::logging;

const API_PATH: &str = "/openaide/relay/v1/";
const CALL_TIMEOUT: Duration = Duration::from_secs(10);

/// A relay key as the relay lists it: a person's name and whether it can manage.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Person {
    pub name: String,
    pub can_manage: bool,
}

/// A key just added, with the line to hand to its person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NewPerson {
    pub person: Person,
    pub key: OwnRelay,
}

/// Why a management call failed, classified for logs and for the user's message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RelayApiError {
    /// The relay does not know the key: it was never issued or was revoked.
    Refused,
    /// The key may use the relay but not manage it.
    CannotManage,
    InvalidName,
    NameTaken,
    NoSuchPerson,
    /// The relay keeps at least one key that can manage.
    LastManager,
    /// The relay has no OpenAIDE management API.
    NoManagement,
    /// No answer, a timeout, or a server failure.
    Unreachable,
    /// An answer this version cannot read.
    Malformed,
}

impl RelayApiError {
    pub(crate) fn class(self) -> &'static str {
        match self {
            Self::Refused => "refused",
            Self::CannotManage => "cannot_manage",
            Self::InvalidName => "invalid_name",
            Self::NameTaken => "name_taken",
            Self::NoSuchPerson => "no_such_person",
            Self::LastManager => "last_manager",
            Self::NoManagement => "no_management",
            Self::Unreachable => "unreachable",
            Self::Malformed => "malformed",
        }
    }
}

/// The management calls, injectable so tests need no relay.
pub(crate) trait RelayApi: Send + Sync {
    /// The person `key` belongs to.
    fn me(&self, key: &OwnRelay) -> Result<Person, RelayApiError>;
    fn people(&self, key: &OwnRelay) -> Result<Vec<Person>, RelayApiError>;
    fn add(&self, key: &OwnRelay, name: &str, can_manage: bool)
        -> Result<NewPerson, RelayApiError>;
    /// Revokes the key named `name`; the relay cuts its connections at once.
    fn revoke(&self, key: &OwnRelay, name: &str) -> Result<(), RelayApiError>;
}

/// Calls the relay over HTTPS, trusting the public certificate authorities.
pub(crate) struct HttpRelayApi;

#[derive(Deserialize)]
struct PeopleAnswer {
    people: Vec<Person>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddAnswer {
    name: String,
    can_manage: bool,
    key: String,
}

#[derive(Deserialize)]
struct ErrorAnswer {
    error: String,
}

impl RelayApi for HttpRelayApi {
    fn me(&self, key: &OwnRelay) -> Result<Person, RelayApiError> {
        let answer = call(key, "me", reqwest::Method::GET, "me", &[])?;
        read(&answer)
    }

    fn people(&self, key: &OwnRelay) -> Result<Vec<Person>, RelayApiError> {
        let answer = call(key, "people", reqwest::Method::GET, "people", &[])?;
        read::<PeopleAnswer>(&answer).map(|answer| answer.people)
    }

    fn add(
        &self,
        key: &OwnRelay,
        name: &str,
        can_manage: bool,
    ) -> Result<NewPerson, RelayApiError> {
        let can_manage = if can_manage { "true" } else { "false" };
        let answer = call(
            key,
            "add_person",
            reqwest::Method::POST,
            "people",
            &[("name", name), ("canManage", can_manage)],
        )?;
        let answer: AddAnswer = read(&answer)?;
        Ok(NewPerson {
            person: Person {
                name: answer.name,
                can_manage: answer.can_manage,
            },
            key: OwnRelay::parse(&answer.key).map_err(|_| RelayApiError::Malformed)?,
        })
    }

    fn revoke(&self, key: &OwnRelay, name: &str) -> Result<(), RelayApiError> {
        call(
            key,
            "revoke_person",
            reqwest::Method::DELETE,
            "people",
            &[("name", name)],
        )
        .map(|_| ())
    }
}

fn read<T: serde::de::DeserializeOwned>(answer: &[u8]) -> Result<T, RelayApiError> {
    serde_json::from_slice(answer).map_err(|_| RelayApiError::Malformed)
}

/// One call, logged at its start and end with metadata only: never the host, the
/// key, or a person's name.
fn call(
    key: &OwnRelay,
    operation: &'static str,
    method: reqwest::Method,
    path: &str,
    query: &[(&str, &str)],
) -> Result<Vec<u8>, RelayApiError> {
    let started = Instant::now();
    logging::info(
        "remote_relay_api_started",
        json!({ "operation": operation }),
    );
    let mut url: reqwest::Url = (**key.url()).clone();
    url.set_path(&format!("{API_PATH}{path}"));
    url.set_query(None);
    if !query.is_empty() {
        url.query_pairs_mut().extend_pairs(query);
    }
    let outcome = send(key, method, url);
    let (status, result) = match outcome {
        Ok((status, body)) => (Some(status), classify(status, body)),
        Err(error) => (None, Err(error)),
    };
    let fields = json!({
        "operation": operation,
        "outcome": if result.is_ok() { "succeeded" } else { "failed" },
        "error": result.as_ref().err().map(|error| error.class()),
        "http_status": status,
        "duration_ms": u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    });
    match &result {
        Ok(_) => logging::info("remote_relay_api_completed", fields),
        Err(_) => logging::warn("remote_relay_api_completed", fields),
    }
    result
}

/// Sends on a runtime of its own: callers are plain threads, or blocking tasks of
/// the edge's runtime.
fn send(
    key: &OwnRelay,
    method: reqwest::Method,
    url: reqwest::Url,
) -> Result<(u16, Vec<u8>), RelayApiError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| RelayApiError::Unreachable)?;
    runtime.block_on(async move {
        let tls = iroh::tls::CaTlsConfig::default()
            .client_config(iroh::tls::default_provider())
            .map_err(|_| RelayApiError::Unreachable)?;
        let client = reqwest::Client::builder()
            .tls_backend_preconfigured(tls)
            .timeout(CALL_TIMEOUT)
            .build()
            .map_err(|_| RelayApiError::Unreachable)?;
        let response = client
            .request(method, url)
            .bearer_auth(key.password())
            .send()
            .await
            .map_err(|_| RelayApiError::Unreachable)?;
        let status = response.status().as_u16();
        let body = response
            .bytes()
            .await
            .map_err(|_| RelayApiError::Unreachable)?;
        Ok((status, body.to_vec()))
    })
}

fn classify(status: u16, body: Vec<u8>) -> Result<Vec<u8>, RelayApiError> {
    if (200..300).contains(&status) {
        return Ok(body);
    }
    let code = serde_json::from_slice::<ErrorAnswer>(&body)
        .map(|answer| answer.error)
        .unwrap_or_default();
    Err(match (status, code.as_str()) {
        (401, _) => RelayApiError::Refused,
        (403, _) => RelayApiError::CannotManage,
        (_, "invalid_name") => RelayApiError::InvalidName,
        (_, "name_taken") => RelayApiError::NameTaken,
        (_, "no_such_person") => RelayApiError::NoSuchPerson,
        (_, "last_manager") => RelayApiError::LastManager,
        // Any other `404` comes from a relay that does not serve the API at all.
        (404, _) => RelayApiError::NoManagement,
        (500.., _) => RelayApiError::Unreachable,
        _ => RelayApiError::Malformed,
    })
}

#[cfg(test)]
#[path = "relay_api_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "relay_api_fake.rs"]
pub(crate) mod fake;
