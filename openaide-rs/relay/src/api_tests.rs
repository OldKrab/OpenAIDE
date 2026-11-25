use std::sync::Arc;

use http::{Method, StatusCode};
use serde_json::json;

use super::*;
use crate::access::RelayAccess;
use crate::config::Host;
use crate::keys::{KeyStore, Person};

struct Fixture {
    _dir: tempfile::TempDir,
    store: Arc<KeyStore>,
    api: ManagementApi,
    owner: Person,
    member: Person,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().expect("temporary directory");
    let store = Arc::new(KeyStore::open(dir.path()));
    let owner = store.add("owner", true).expect("added");
    let member = store.add("Pat", false).expect("added");
    let access = Arc::new(RelayAccess::new(store.clone()));
    let host = Host::parse("relay.example.com").expect("host");
    let api = ManagementApi::new(store.clone(), access, host, 443);
    Fixture {
        _dir: dir,
        store,
        api,
        owner,
        member,
    }
}

fn call(
    api: &ManagementApi,
    method: Method,
    path: &str,
    query: Option<&str>,
    token: Option<&str>,
) -> ApiResponse {
    let authorization = token.map(|token| format!("Bearer {token}"));
    api.handle(&ApiRequest {
        method: &method,
        path,
        query,
        authorization: authorization.as_deref(),
    })
}

fn error(status: StatusCode, code: &str) -> ApiResponse {
    ApiResponse {
        status,
        body: json!({ "error": code }),
    }
}

#[test]
fn me_names_the_caller() {
    let f = fixture();
    let owner = call(&f.api, Method::GET, ME_PATH, None, Some(&f.owner.token));
    assert_eq!(owner.status, StatusCode::OK);
    assert_eq!(owner.body, json!({ "name": "owner", "canManage": true }));
    let member = call(&f.api, Method::GET, ME_PATH, None, Some(&f.member.token));
    assert_eq!(member.body, json!({ "name": "Pat", "canManage": false }));
}

#[test]
fn every_route_refuses_a_missing_or_unknown_key() {
    let f = fixture();
    let unknown = "0".repeat(48);
    for (method, path, query) in [
        (Method::GET, ME_PATH, None),
        (Method::GET, PEOPLE_PATH, None),
        (Method::POST, PEOPLE_PATH, Some("name=Olga")),
        (Method::DELETE, PEOPLE_PATH, Some("name=Pat")),
    ] {
        for token in [None, Some(unknown.as_str()), Some("")] {
            assert_eq!(
                call(&f.api, method.clone(), path, query, token),
                error(StatusCode::UNAUTHORIZED, "unknown_key"),
                "{method} {path} with {token:?}"
            );
        }
    }
    let basic = f.api.handle(&ApiRequest {
        method: &Method::GET,
        path: ME_PATH,
        query: None,
        authorization: Some(&format!("Basic {}", f.owner.token)),
    });
    assert_eq!(basic.status, StatusCode::UNAUTHORIZED);
    let lowercase = f.api.handle(&ApiRequest {
        method: &Method::GET,
        path: ME_PATH,
        query: None,
        authorization: Some(&format!("bearer {}", f.owner.token)),
    });
    assert_eq!(lowercase.status, StatusCode::OK, "the scheme ignores case");
    assert_eq!(f.store.people().expect("read").len(), 2, "nothing changed");
}

#[test]
fn people_routes_need_a_key_that_can_manage() {
    let f = fixture();
    for (method, query) in [
        (Method::GET, None),
        (Method::POST, Some("name=Olga")),
        (Method::DELETE, Some("name=owner")),
    ] {
        assert_eq!(
            call(&f.api, method, PEOPLE_PATH, query, Some(&f.member.token)),
            error(StatusCode::FORBIDDEN, "cannot_manage")
        );
    }
    assert_eq!(f.store.people().expect("read").len(), 2, "nothing changed");
}

#[test]
fn people_lists_everyone_in_creation_order() {
    let f = fixture();
    let response = call(&f.api, Method::GET, PEOPLE_PATH, None, Some(&f.owner.token));
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(
        response.body,
        json!({ "people": [
            { "name": "owner", "canManage": true },
            { "name": "Pat", "canManage": false },
        ] })
    );
}

#[test]
fn adding_returns_a_working_key_line() {
    let f = fixture();
    let response = call(
        &f.api,
        Method::POST,
        PEOPLE_PATH,
        Some("name=Olga%20P%C3%A9rez&canManage=true"),
        Some(&f.owner.token),
    );
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.body["name"], "Olga Pérez");
    assert_eq!(response.body["canManage"], true);
    let line = response.body["key"].as_str().expect("key line");
    let token = line
        .strip_prefix("https://relay.example.com/?token=")
        .expect("line for this relay");
    let olga = f
        .store
        .person_for_token(token)
        .expect("read")
        .expect("stored");
    assert!(olga.can_manage);

    let plain = call(
        &f.api,
        Method::POST,
        PEOPLE_PATH,
        Some("name=Kim"),
        Some(&f.owner.token),
    );
    assert_eq!(
        plain.body["canManage"], false,
        "canManage defaults to false"
    );
    let explicit = call(
        &f.api,
        Method::POST,
        PEOPLE_PATH,
        Some("name=Lee&canManage=false"),
        Some(&f.owner.token),
    );
    assert_eq!(explicit.body["canManage"], false);
}

#[test]
fn adding_refuses_a_bad_or_taken_name() {
    let f = fixture();
    let long = format!("name={}", "x".repeat(41));
    for query in [
        None,
        Some("name="),
        Some("name=%20%20"),
        Some("name=a%0Ab"),
        Some(long.as_str()),
    ] {
        assert_eq!(
            call(
                &f.api,
                Method::POST,
                PEOPLE_PATH,
                query,
                Some(&f.owner.token)
            ),
            error(StatusCode::BAD_REQUEST, "invalid_name"),
            "{query:?}"
        );
    }
    assert_eq!(
        call(
            &f.api,
            Method::POST,
            PEOPLE_PATH,
            Some("name=PAT"),
            Some(&f.owner.token)
        ),
        error(StatusCode::CONFLICT, "name_taken")
    );
    assert_eq!(
        call(
            &f.api,
            Method::POST,
            PEOPLE_PATH,
            Some("name=Olga&canManage=yes"),
            Some(&f.owner.token)
        ),
        error(StatusCode::BAD_REQUEST, "invalid_request")
    );
    assert_eq!(f.store.people().expect("read").len(), 2, "nothing added");
}

#[test]
fn deleting_revokes_but_never_the_last_manager() {
    let f = fixture();
    assert_eq!(
        call(
            &f.api,
            Method::DELETE,
            PEOPLE_PATH,
            Some("name=Nobody"),
            Some(&f.owner.token)
        ),
        error(StatusCode::NOT_FOUND, "no_such_person")
    );
    assert_eq!(
        call(
            &f.api,
            Method::DELETE,
            PEOPLE_PATH,
            Some("name=owner"),
            Some(&f.owner.token)
        ),
        error(StatusCode::CONFLICT, "last_manager")
    );
    let revoked = call(
        &f.api,
        Method::DELETE,
        PEOPLE_PATH,
        Some("name=pat"),
        Some(&f.owner.token),
    );
    assert_eq!(revoked.status, StatusCode::OK);
    assert_eq!(revoked.body, json!({}));
    assert_eq!(
        call(&f.api, Method::GET, ME_PATH, None, Some(&f.member.token)),
        error(StatusCode::UNAUTHORIZED, "unknown_key"),
        "the revoked key stops working"
    );
}

#[test]
fn an_unknown_route_is_not_found() {
    let f = fixture();
    assert_eq!(
        call(&f.api, Method::PUT, PEOPLE_PATH, None, Some(&f.owner.token)),
        error(StatusCode::NOT_FOUND, "not_found")
    );
}
