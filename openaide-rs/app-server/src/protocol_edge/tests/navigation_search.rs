//! `taskNavigation/search` at the gateway: it answers through the shared gateway, which
//! runs the scan outside the protocol lock, and only for an initialized client.

use openaide_app_server_protocol::methods::TASK_NAVIGATION_SEARCH;
use openaide_app_server_protocol::task::TaskNavigationSearchParams;

use super::*;
use crate::navigation_search::NavigationSearch;

fn search_request(text: &str) -> InboundProtocolMessage {
    request(
        "search-1",
        TASK_NAVIGATION_SEARCH,
        TaskNavigationSearchParams {
            text: text.to_string(),
            project_ids: None,
        },
    )
}

fn searchable_gateway() -> (RpcGateway, Store) {
    let (gateway, store) = gateway_with_project_context_and_store();
    let search = NavigationSearch::new(
        std::sync::Arc::new(TaskNavigationStore::new(store.clone())),
        store.clone(),
    );
    (
        gateway.with_navigation_search(std::sync::Arc::new(search)),
        store,
    )
}

#[test]
fn search_answers_an_initialized_client() {
    let (mut gateway, store) = searchable_gateway();
    let mut task = client_new_task_record("task-search", "client-1");
    task.lifecycle = crate::storage::records::TaskLifecycle::Open;
    task.title = crate::storage::records::TaskTitleState::from_title(
        crate::storage::records::TaskTitle::new(
            "Walrus migration",
            crate::storage::records::TaskTitleSource::User,
        ),
    );
    store.write_task(&task).unwrap();
    let connection_id = ConnectionId::new("conn-1");
    initialize_client(&mut gateway, connection_id.clone());
    let gateway = SharedRpcGateway::new(gateway);

    let reply = response_value(gateway.handle_inbound(
        connection_id,
        search_request("walrus"),
        AppServerTime(10),
    ));

    let hits = reply["result"]["hits"].as_array().unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["section"], "tasks");
    assert_eq!(hits[0]["row"]["kind"], "task");
    assert_eq!(hits[0]["titleHighlights"], serde_json::json!([[0, 6]]));
    assert_eq!(reply["result"]["truncated"], false);
}

#[test]
fn search_before_initialize_is_refused() {
    let (gateway, _store) = searchable_gateway();
    let gateway = SharedRpcGateway::new(gateway);

    let outcome = gateway.handle_inbound(
        ConnectionId::new("conn-uninitialized"),
        search_request("walrus"),
        AppServerTime(10),
    );

    assert!(!matches!(
        outcome,
        GatewayOutcome::Respond {
            response: GatewayResponse::Result(_),
            ..
        }
    ));
}

#[test]
fn search_with_malformed_params_is_an_invalid_request() {
    let (mut gateway, _store) = searchable_gateway();
    let connection_id = ConnectionId::new("conn-1");
    initialize_client(&mut gateway, connection_id.clone());
    let gateway = SharedRpcGateway::new(gateway);

    let outcome = gateway.handle_inbound(
        connection_id,
        request("search-2", TASK_NAVIGATION_SEARCH, serde_json::json!({})),
        AppServerTime(10),
    );

    assert!(!matches!(
        outcome,
        GatewayOutcome::Respond {
            response: GatewayResponse::Result(_),
            ..
        }
    ));
}
