//! `task/adoptNativeSession` and `task/reloadNativeSession` outside the protocol lock.
//!
//! Both wait on an Agent `session/load` (hundreds of milliseconds to seconds) and then
//! project and persist the replay. Holding the one protocol mutex for that long delayed every
//! other client's requests and every delivery. The lock is taken only to admit the request
//! and to queue its reply; the Task commit itself runs on the workflow's own storage gates,
//! and its Task and Navigation publications reach the outbox through the usual publishers.
//!
//! Ordering (ADR-0086): the reply joins the connection's sequence under the protocol lock,
//! after the Navigation frames the commit marked (`handle_queued` appends the flush), so a
//! mutation's frames still precede its reply. Concurrent duplicate adoptions of one session
//! serialize inside the workflow; see `TaskProductApi::adopt_native_session_as_task`.

use std::sync::Arc;
use std::time::Instant;

use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::ids::ClientInstanceId;
use openaide_app_server_protocol::methods::TASK_ADOPT_NATIVE_SESSION;
use openaide_app_server_protocol::task::{
    TaskAdoptNativeSessionParams, TaskAdoptNativeSessionResult, TaskReloadNativeSessionParams,
    TaskReloadNativeSessionResult,
};
use serde_json::json;

use crate::client_lifecycle::{AppServerTime, ConnectionId};
use crate::tasks::product_api::{TaskAdoptNativeSessionWorkflow, TaskOpenWorkflow};

use super::super::{responses, InboundProtocolMessage};
use super::{Handled, SharedRpcGateway};

/// The captured, lock-free half of one request.
enum Job {
    Adopt(
        Arc<dyn TaskAdoptNativeSessionWorkflow>,
        TaskAdoptNativeSessionParams,
    ),
    Reload(
        Arc<dyn TaskOpenWorkflow>,
        ClientInstanceId,
        TaskReloadNativeSessionParams,
    ),
}

enum Done {
    Adopted(Result<Box<openaide_app_server_protocol::snapshot::TaskSnapshot>, ProtocolError>),
    Reloaded(Result<(), ProtocolError>),
}

impl SharedRpcGateway {
    pub(super) fn handle_native_session_without_protocol_lock(
        &self,
        connection_id: ConnectionId,
        message: InboundProtocolMessage,
        now: AppServerTime,
        completion_clock: impl FnOnce() -> AppServerTime,
    ) -> Handled<'_> {
        let admitted_at = Instant::now();
        let (owner, job) = {
            let mut gateway = self.lock();
            // Keep routing's initialization and shutdown rejections.
            let Some(client) = (gateway.update_shutdown.is_none())
                .then(|| gateway.client_hub.context_for_connection(&connection_id))
                .flatten()
            else {
                let outcome = gateway.handle_inbound(connection_id, message, now);
                return (gateway, outcome);
            };
            gateway
                .client_hub
                .observe_connection_activity(&connection_id, now);
            let InboundProtocolMessage::ClientRequest {
                id,
                method,
                params,
                meta,
                ..
            } = &message
            else {
                unreachable!("the native session path only handles client requests")
            };
            let parsed = if method == TASK_ADOPT_NATIVE_SESSION {
                serde_json::from_value::<TaskAdoptNativeSessionParams>(params.clone())
                    .map(|params| Job::Adopt(gateway.task_adopt_native_session.clone(), params))
            } else {
                serde_json::from_value::<TaskReloadNativeSessionParams>(params.clone()).map(
                    |params| {
                        Job::Reload(
                            gateway.task_open.clone(),
                            client.client_instance_id.clone(),
                            params,
                        )
                    },
                )
            };
            match parsed {
                Ok(job) => (client.client_instance_id, job),
                Err(error) => {
                    let outcome = gateway.error(
                        connection_id,
                        id.clone(),
                        meta.clone(),
                        responses::invalid_params(error),
                    );
                    return (gateway, outcome);
                }
            }
        };
        let InboundProtocolMessage::ClientRequest {
            id, method, meta, ..
        } = message
        else {
            unreachable!("the native session path only handles client requests")
        };
        let operation = method.clone();
        let done = match job {
            Job::Adopt(workflow, params) => {
                Done::Adopted(workflow.adopt_native_session(params).map(Box::new))
            }
            Job::Reload(workflow, client, params) => {
                Done::Reloaded(workflow.reload_native_session_for_client(&client, params))
            }
        };
        let run_ms = admitted_at.elapsed().as_millis();

        let relock_started_at = Instant::now();
        let mut gateway = self.lock();
        let lock_wait_ms = relock_started_at.elapsed().as_millis();
        // A reconnect or detach during the load must not answer a new owner. The commit
        // already happened (or failed) on its own; only the reply is dropped.
        let still_owned = gateway
            .client_hub
            .context_for_connection(&connection_id)
            .is_some_and(|client| client.client_instance_id == owner);
        crate::logging::info(
            "native_session_request_replied",
            json!({
                "operation": operation,
                "connection_id": connection_id.as_str(),
                "outcome": match &done {
                    Done::Adopted(Ok(_)) | Done::Reloaded(Ok(())) => "completed",
                    _ => "failed",
                },
                "client_still_connected": still_owned,
                "off_lock_ms": run_ms,
                "lock_wait_ms": lock_wait_ms,
            }),
        );
        if !still_owned {
            let outcome =
                gateway.error(connection_id, id, meta, responses::not_initialized(method));
            return (gateway, outcome);
        }
        gateway
            .client_hub
            .observe_connection_activity(&connection_id, completion_clock());
        let outcome = match done {
            Done::Adopted(Ok(task)) => gateway.result::<TaskAdoptNativeSessionResult>(
                connection_id,
                id,
                meta,
                TaskAdoptNativeSessionResult { task: *task },
            ),
            Done::Reloaded(Ok(())) => gateway.result::<TaskReloadNativeSessionResult>(
                connection_id,
                id,
                meta,
                TaskReloadNativeSessionResult {},
            ),
            Done::Adopted(Err(error)) | Done::Reloaded(Err(error)) => {
                gateway.error(connection_id, id, meta, error)
            }
        };
        (gateway, outcome)
    }
}
