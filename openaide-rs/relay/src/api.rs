//! The management API that OpenAIDE's App Server calls with a relay key that can manage.
//!
//! Contract, under [`BASE`], with `Authorization: Bearer <token>`, JSON bodies with
//! camelCase keys, and errors as `{"error":"<code>"}`:
//!
//! - `GET /me` → `{"name","canManage"}`; 401 `unknown_key`.
//! - `GET /people` → `{"people":[{"name","canManage"}]}` in creation order; 401; 403
//!   `cannot_manage`.
//! - `POST /people?name=<name>&canManage=true|false` → `{"name","canManage","key"}`; 400
//!   `invalid_name`; 409 `name_taken`; 401; 403.
//! - `DELETE /people?name=<name>` → `{}`; 404 `no_such_person`; 409 `last_manager`; 401;
//!   403. Also cuts that key's live relay connections.
//!
//! iroh's request handlers are synchronous and cannot read a request body, so requests
//! carry everything in the query and headers. [`ManagementApi::handle`] is a pure function
//! of that request apart from the key store, which keeps the contract testable without a
//! server.

use std::sync::Arc;
use std::time::Instant;

use http::{header, Method, StatusCode};
use http_body_util::Full;
use hyper::body::Bytes;
use iroh_relay::server::http_server::{BytesBody, HyperResult};
use iroh_relay::server::Handlers;
use serde_json::{json, Value};

use crate::access::RelayAccess;
use crate::config::{key_line, Host};
use crate::keys::{AddError, KeyStore, Person, RevokeError, StoreError};
use crate::log;

pub const BASE: &str = "/openaide/relay/v1";
pub const ME_PATH: &str = "/openaide/relay/v1/me";
pub const PEOPLE_PATH: &str = "/openaide/relay/v1/people";

/// The parts of an HTTP request the API reads.
#[derive(Debug, Clone, Copy)]
pub struct ApiRequest<'a> {
    pub method: &'a Method,
    pub path: &'a str,
    pub query: Option<&'a str>,
    /// The `Authorization` header, if it is valid text.
    pub authorization: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ApiResponse {
    pub status: StatusCode,
    pub body: Value,
}

impl ApiResponse {
    fn ok(body: Value) -> Self {
        Self {
            status: StatusCode::OK,
            body,
        }
    }

    fn error(status: StatusCode, code: &str) -> Self {
        Self {
            status,
            body: json!({ "error": code }),
        }
    }
}

#[derive(Debug)]
pub struct ManagementApi {
    store: Arc<KeyStore>,
    access: Arc<RelayAccess>,
    host: Host,
    port: u16,
}

impl ManagementApi {
    /// `host` and `port` are the relay's public address, which new key lines carry.
    pub fn new(store: Arc<KeyStore>, access: Arc<RelayAccess>, host: Host, port: u16) -> Self {
        Self {
            store,
            access,
            host,
            port,
        }
    }

    pub fn handle(&self, request: &ApiRequest<'_>) -> ApiResponse {
        let started = Instant::now();
        let caller = match self.caller(request.authorization) {
            Ok(caller) => caller,
            Err(response) => {
                Self::report(request, &response, None, started);
                return response;
            }
        };
        let response = match (request.method, request.path) {
            (&Method::GET, ME_PATH) => ApiResponse::ok(person_json(&caller)),
            (_, PEOPLE_PATH) if !caller.can_manage => {
                ApiResponse::error(StatusCode::FORBIDDEN, "cannot_manage")
            }
            (&Method::GET, PEOPLE_PATH) => self.list(),
            (&Method::POST, PEOPLE_PATH) => self.add(request.query),
            (&Method::DELETE, PEOPLE_PATH) => self.revoke(request.query),
            _ => ApiResponse::error(StatusCode::NOT_FOUND, "not_found"),
        };
        // Reads that succeed are the healthy path and stay quiet.
        if request.method != Method::GET || !response.status.is_success() {
            Self::report(request, &response, Some(&caller), started);
        }
        response
    }

    /// Registers every route on iroh's relay service.
    ///
    /// iroh calls handlers synchronously on the connection's task, so a change briefly
    /// blocks a runtime thread on the key file's lock and write. Changes are rare and the
    /// file is small, so this is accepted instead of moving the API off iroh's handlers.
    pub fn register(self: &Arc<Self>, handlers: &mut Handlers) {
        let routes = [
            (Method::GET, ME_PATH),
            (Method::GET, PEOPLE_PATH),
            (Method::POST, PEOPLE_PATH),
            (Method::DELETE, PEOPLE_PATH),
        ];
        for (method, path) in routes {
            let api = self.clone();
            handlers.insert(
                (method, path),
                Box::new(move |request, response| {
                    let authorization = request
                        .headers()
                        .get(header::AUTHORIZATION)
                        .and_then(|value| value.to_str().ok());
                    let answer = api.handle(&ApiRequest {
                        method: request.method(),
                        path: request.uri().path(),
                        query: request.uri().query(),
                        authorization,
                    });
                    json_response(response, answer)
                }),
            );
        }
    }

    fn caller(&self, authorization: Option<&str>) -> Result<Person, ApiResponse> {
        let token = authorization
            .and_then(|value| value.split_once(' '))
            .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("Bearer"))
            .map(|(_, token)| token.trim())
            .filter(|token| !token.is_empty());
        let unknown = || ApiResponse::error(StatusCode::UNAUTHORIZED, "unknown_key");
        match token.map(|token| self.store.person_for_token(token)) {
            None | Some(Ok(None)) => Err(unknown()),
            Some(Ok(Some(person))) => Ok(person),
            Some(Err(error)) => Err(store_failure(&error)),
        }
    }

    fn list(&self) -> ApiResponse {
        match self.store.people() {
            Ok(people) => ApiResponse::ok(json!({
                "people": people.iter().map(person_json).collect::<Vec<_>>(),
            })),
            Err(error) => store_failure(&error),
        }
    }

    fn add(&self, query: Option<&str>) -> ApiResponse {
        let Some(name) = query_value(query, "name") else {
            return ApiResponse::error(StatusCode::BAD_REQUEST, "invalid_name");
        };
        let can_manage = match query_value(query, "canManage").as_deref() {
            None | Some("false") => false,
            Some("true") => true,
            // Not in the agreed contract, which lists only `invalid_name` for 400.
            Some(_) => return ApiResponse::error(StatusCode::BAD_REQUEST, "invalid_request"),
        };
        match self.store.add(&name, can_manage) {
            Ok(person) => {
                let mut body = person_json(&person);
                body["key"] = json!(key_line(&self.host, self.port, &person.token));
                ApiResponse::ok(body)
            }
            Err(AddError::InvalidName) => {
                ApiResponse::error(StatusCode::BAD_REQUEST, "invalid_name")
            }
            Err(AddError::NameTaken) => ApiResponse::error(StatusCode::CONFLICT, "name_taken"),
            Err(AddError::Store(error)) => store_failure(&error),
        }
    }

    fn revoke(&self, query: Option<&str>) -> ApiResponse {
        let Some(name) = query_value(query, "name") else {
            return ApiResponse::error(StatusCode::NOT_FOUND, "no_such_person");
        };
        match self.store.revoke(&name) {
            Ok(_) => {
                self.access.cut_revoked();
                ApiResponse::ok(json!({}))
            }
            Err(RevokeError::NoSuchPerson) => {
                ApiResponse::error(StatusCode::NOT_FOUND, "no_such_person")
            }
            Err(RevokeError::LastManager) => {
                ApiResponse::error(StatusCode::CONFLICT, "last_manager")
            }
            Err(RevokeError::Store(error)) => store_failure(&error),
        }
    }

    fn report(
        request: &ApiRequest<'_>,
        response: &ApiResponse,
        caller: Option<&Person>,
        started: Instant,
    ) {
        log::event(
            "api.request",
            json!({
                "method": request.method.as_str(),
                "route": request.path.strip_prefix(BASE).unwrap_or("other"),
                "status": response.status.as_u16(),
                "error": response.body.get("error"),
                "keyId": caller.map(Person::key_id),
                "durationMs": log::millis_since(started),
            }),
        );
    }
}

fn person_json(person: &Person) -> Value {
    json!({ "name": person.name, "canManage": person.can_manage })
}

fn store_failure(error: &StoreError) -> ApiResponse {
    log::event(
        "keys.read",
        json!({ "outcome": "error", "errorClass": error.class() }),
    );
    ApiResponse::error(StatusCode::INTERNAL_SERVER_ERROR, "store_unavailable")
}

/// The first value of `name` in a URL query, percent-decoded.
fn query_value(query: Option<&str>, name: &str) -> Option<String> {
    url::form_urlencoded::parse(query.unwrap_or("").as_bytes())
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
}

fn json_response(
    builder: http::response::Builder,
    answer: ApiResponse,
) -> HyperResult<http::Response<BytesBody>> {
    let body: BytesBody = Box::new(Full::new(Bytes::from(answer.body.to_string())));
    Ok(builder
        .status(answer.status)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::CACHE_CONTROL, "no-store")
        .body(body)?)
}

#[cfg(test)]
#[path = "api_tests.rs"]
mod tests;
