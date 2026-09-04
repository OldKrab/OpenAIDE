use std::collections::HashMap;
use std::time::{Duration, Instant};

use openaide_app_server_protocol::ids::ClientInstanceId;
use openaide_app_server_protocol::server_requests::SecretReadResponse;

use crate::client_lifecycle::{AppServerTime, Delivery};
use crate::logging;
use crate::protocol::errors::RuntimeError;
use crate::server_requests::ServerRequestRuntime;

use super::AgentSecretResolver;

const SECRET_READ_TIMEOUT: Duration = Duration::from_secs(15);

/// Resolves process-launch secrets only from the App Shell that initiated authentication.
pub(crate) struct ClientSecretResolver {
    server_requests: ServerRequestRuntime,
    client_instance_id: ClientInstanceId,
    delivery: Delivery,
}

impl ClientSecretResolver {
    pub(crate) fn new(
        server_requests: ServerRequestRuntime,
        client_instance_id: ClientInstanceId,
        delivery: Delivery,
    ) -> Self {
        Self {
            server_requests,
            client_instance_id,
            delivery,
        }
    }

    fn resolve(&self, key: String, label: String) -> Result<String, RuntimeError> {
        let opened = self.server_requests.open_secret_read_request(
            self.client_instance_id.clone(),
            self.delivery.clone(),
            key,
            Some(label),
            AppServerTime::now(),
        )?;
        let value = self
            .server_requests
            .wait_client_response(&opened.request_id, SECRET_READ_TIMEOUT)?;
        let response: SecretReadResponse = serde_json::from_value(value)
            .map_err(|_| RuntimeError::InvalidParams("secret/read response".to_string()))?;
        response
            .value
            .ok_or_else(|| RuntimeError::NotReady("missing secure value".to_string()))
    }
}

impl AgentSecretResolver for ClientSecretResolver {
    fn resolve_secret_env(
        &self,
        agent_id: &str,
        names: &[String],
    ) -> Result<HashMap<String, String>, RuntimeError> {
        let started_at = Instant::now();
        logging::info(
            "agent_auth_secret_read_started",
            serde_json::json!({
                "client_instance_id": self.client_instance_id.as_str(),
                "secret_count": names.len(),
            }),
        );
        let result: Result<HashMap<String, String>, RuntimeError> = names
            .iter()
            .map(|name| {
                self.resolve(
                    format!("openaide.agent.{agent_id}.env.{name}"),
                    format!("{agent_id} environment variable {name}"),
                )
                .map(|value| (name.clone(), value))
            })
            .collect();
        logging::info(
            "agent_auth_secret_read_completed",
            serde_json::json!({
                "client_instance_id": self.client_instance_id.as_str(),
                "secret_count": names.len(),
                "outcome": if result.is_ok() { "success" } else { "failure" },
                "duration_ms": started_at.elapsed().as_millis(),
            }),
        );
        result
    }

    fn resolve_mcp_servers(
        &self,
        _capabilities: &super::acp_schema::McpCapabilities,
    ) -> Result<Vec<super::acp_schema::McpServer>, RuntimeError> {
        Err(RuntimeError::CapabilityMissing(
            "client_auth_secret_resolver:mcp".to_string(),
        ))
    }
}
