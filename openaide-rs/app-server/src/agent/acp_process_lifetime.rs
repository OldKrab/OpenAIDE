use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agent_client_protocol::{AcpAgent, Agent, Channel, Client, ConnectTo, RawJsonRpcMessage};
use tokio::sync::Notify;
use tokio::time::Instant;

use crate::agent::acp_schema::{RequestId, Response};

#[derive(Clone, Copy)]
pub(super) struct ProcessIdleTimeouts {
    pub(super) short: Duration,
    pub(super) long: Duration,
}

impl Default for ProcessIdleTimeouts {
    fn default() -> Self {
        Self {
            short: Duration::from_secs(60),
            long: Duration::from_secs(30 * 60),
        }
    }
}

/// One process owns retention, independent of session count and UI visibility. Only
/// envelope metadata is retained: payloads never enter lifetime state or diagnostics.
#[derive(Clone)]
pub(super) struct AcpProcessLifetime(Arc<Lifetime>);

struct Lifetime {
    state: Mutex<State>,
    changed: Notify,
    timeouts: ProcessIdleTimeouts,
    operation_id: String,
}

struct State {
    last_activity: Instant,
    long_retention: bool,
    stopping: bool,
    admitted_operations: usize,
    client_requests: HashSet<RequestId>,
    agent_requests: HashSet<RequestId>,
}

impl AcpProcessLifetime {
    pub(super) fn new(timeouts: ProcessIdleTimeouts) -> Self {
        Self(Arc::new(Lifetime {
            state: Mutex::new(State {
                last_activity: Instant::now(),
                long_retention: false,
                stopping: false,
                admitted_operations: 0,
                client_requests: HashSet::new(),
                agent_requests: HashSet::new(),
            }),
            changed: Notify::new(),
            timeouts,
            operation_id: uuid::Uuid::new_v4().to_string(),
        }))
    }

    pub(super) fn same_process(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    pub(super) fn is_stopping(&self) -> bool {
        self.0.state.lock().expect("ACP lifetime poisoned").stopping
    }

    /// Admission and idle retirement share a lock. A caller either owns the
    /// process through dispatch/completion or must reacquire another generation.
    pub(super) fn acquire(&self) -> Option<ProcessOperation> {
        let mut state = self.0.state.lock().expect("ACP lifetime poisoned");
        if state.stopping {
            return None;
        }
        state.admitted_operations += 1;
        Some(ProcessOperation {
            lifetime: self.clone(),
        })
    }

    pub(super) fn operation_id(&self) -> &str {
        &self.0.operation_id
    }

    pub(super) fn transport(&self, agent: AcpAgent) -> impl ConnectTo<Client> {
        RetainedAgent {
            agent,
            lifetime: self.clone(),
        }
    }

    fn observe(&self, message: &RawJsonRpcMessage, from_client: bool) {
        let mut state = self.0.state.lock().expect("ACP lifetime poisoned");
        state.last_activity = Instant::now();
        match message {
            RawJsonRpcMessage::Request(request) => {
                if from_client {
                    state.client_requests.insert(request.id.clone());
                } else {
                    state.agent_requests.insert(request.id.clone());
                }
                if from_client
                    && !state.long_retention
                    && matches!(
                        request.method.as_ref(),
                        "session/new"
                            | "session/prompt"
                            | "session/fork"
                            | "session/set_config_option"
                            | "session/set_mode"
                            | "session/set_model"
                            | "session/delete"
                    )
                {
                    state.long_retention = true;
                    crate::logging::info(
                        "acp_agent_process_retention_promoted",
                        serde_json::json!({
                            "operation": "agent/process_lifetime",
                            "operation_id": self.operation_id(),
                            "idle_timeout_ms": self.0.timeouts.long.as_millis(),
                        }),
                    );
                }
            }
            RawJsonRpcMessage::Response(response) => {
                let (Response::Result { id, .. } | Response::Error { id, .. }) = response;
                if from_client {
                    state.agent_requests.remove(id);
                } else {
                    state.client_requests.remove(id);
                }
            }
            RawJsonRpcMessage::Notification(_) => {}
        }
        drop(state);
        self.0.changed.notify_one();
    }

    /// Atomically claims expiration so a caller cannot reuse a retiring process.
    /// Pending wire requests suspend expiration; request cancellation/deadlines
    /// remain owned by the operation that issued them.
    pub(super) async fn expired(&self) -> Duration {
        loop {
            let changed = self.0.changed.notified();
            let deadline = {
                let mut state = self.0.state.lock().expect("ACP lifetime poisoned");
                let timeout = if state.long_retention {
                    self.0.timeouts.long
                } else {
                    self.0.timeouts.short
                };
                if state.admitted_operations > 0
                    || !state.client_requests.is_empty()
                    || !state.agent_requests.is_empty()
                {
                    None
                } else {
                    let deadline = state.last_activity + timeout;
                    if Instant::now() >= deadline {
                        state.stopping = true;
                        return timeout;
                    }
                    Some(deadline)
                }
            };
            match deadline {
                Some(deadline) => tokio::select! {
                    _ = changed => {},
                    _ = tokio::time::sleep_until(deadline) => {},
                },
                None => changed.await,
            }
        }
    }
}

pub(super) struct ProcessOperation {
    lifetime: AcpProcessLifetime,
}

impl Drop for ProcessOperation {
    fn drop(&mut self) {
        let mut state = self.lifetime.0.state.lock().expect("ACP lifetime poisoned");
        state.admitted_operations -= 1;
        drop(state);
        self.lifetime.0.changed.notify_one();
    }
}

struct RetainedAgent {
    agent: AcpAgent,
    lifetime: AcpProcessLifetime,
}

impl ConnectTo<Client> for RetainedAgent {
    async fn connect_to(self, client: impl ConnectTo<Agent>) -> agent_client_protocol::Result<()> {
        let (agent_channel, agent_future) =
            ConnectTo::<Client>::into_channel_and_future(self.agent);
        let (client_channel, client_future) = client.into_channel_and_future();
        // Inspect complete frames without rewriting IDs, ordering, batches, or
        // unknown methods. The SDK still owns process-group termination on drop.
        let bridge = Channel::bridge_with_inspection(
            client_channel,
            agent_channel,
            |message| {
                self.lifetime.observe(message, true);
                Ok(())
            },
            |message| {
                self.lifetime.observe(message, false);
                Ok(())
            },
        );
        tokio::select! {
            result = agent_future => result,
            result = async { tokio::try_join!(client_future, bridge).map(|_| ()) } => result,
        }
    }
}
