use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use openaide_app_server_protocol::ids::TaskId;
use openaide_app_server_protocol::server_requests::SecretReadResponse;

use crate::agent::AgentSecretResolver;
use crate::client_lifecycle::AppServerTime;
use crate::protocol::errors::RuntimeError;
use crate::server_requests::ServerRequestRuntime;

use super::TaskProductApi;

const SECRET_READ_TIMEOUT: Duration = Duration::from_secs(5);

impl TaskProductApi {
    pub(super) fn task_secret_resolver(&self, task_id: &str) -> Arc<dyn AgentSecretResolver> {
        task_secret_resolver(&self.server_requests, task_id)
    }
}

pub(crate) fn task_secret_resolver(
    server_requests: &ServerRequestRuntime,
    task_id: &str,
) -> Arc<dyn AgentSecretResolver> {
    Arc::new(TaskSecretResolver {
        server_requests: server_requests.clone(),
        task_id: TaskId::from(task_id.to_string()),
    })
}

struct TaskSecretResolver {
    server_requests: ServerRequestRuntime,
    task_id: TaskId,
}

impl AgentSecretResolver for TaskSecretResolver {
    fn resolve_secret_env(
        &self,
        agent_id: &str,
        names: &[String],
    ) -> Result<HashMap<String, String>, RuntimeError> {
        names
            .iter()
            .map(|name| {
                let opened = self.server_requests.open_task_secret_read_request(
                    self.task_id.clone(),
                    secret_storage_key(agent_id, name),
                    Some(format!("{agent_id} environment variable {name}")),
                    AppServerTime::now(),
                )?;
                let value = self
                    .server_requests
                    .wait_client_response(&opened.request_id, SECRET_READ_TIMEOUT)?;
                let response: SecretReadResponse = serde_json::from_value(value)
                    .map_err(|_| RuntimeError::InvalidParams("secret/read response".to_string()))?;
                let value = response.value.ok_or_else(|| {
                    RuntimeError::NotReady(format!("missing secret environment value {name}"))
                })?;
                Ok((name.clone(), value))
            })
            .collect()
    }
}

fn secret_storage_key(agent_id: &str, name: &str) -> String {
    format!("openaide.agent.{agent_id}.env.{name}")
}
