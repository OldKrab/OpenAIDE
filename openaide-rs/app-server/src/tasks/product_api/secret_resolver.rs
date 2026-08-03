use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use openaide_app_server_protocol::ids::{ProjectId, TaskId};
use openaide_app_server_protocol::server_requests::SecretReadResponse;
use openaide_app_server_protocol::settings::{McpServerConfiguration, McpServerDefinition};

use crate::agent::acp_schema::{
    EnvVariable, HttpHeader, McpCapabilities, McpServer, McpServerHttp, McpServerSse,
    McpServerStdio,
};
use crate::agent::AgentSecretResolver;
use crate::client_lifecycle::AppServerTime;
use crate::protocol::errors::RuntimeError;
use crate::server_requests::ServerRequestRuntime;
use crate::storage::Store;

use super::TaskProductApi;

const SECRET_READ_TIMEOUT: Duration = Duration::from_secs(5);

impl TaskProductApi {
    /// Creates a resolver for session adoption, before the Task record exists.
    pub(super) fn task_secret_resolver_for_project(
        &self,
        task_id: &str,
        project_id: ProjectId,
    ) -> Arc<dyn AgentSecretResolver> {
        task_secret_resolver_with_project(
            &self.server_requests,
            &self.store,
            task_id,
            Some(project_id),
        )
    }
}

pub(crate) fn task_secret_resolver(
    server_requests: &ServerRequestRuntime,
    store: &Store,
    task_id: &str,
) -> Arc<dyn AgentSecretResolver> {
    task_secret_resolver_with_project(server_requests, store, task_id, None)
}

fn task_secret_resolver_with_project(
    server_requests: &ServerRequestRuntime,
    store: &Store,
    task_id: &str,
    project_id: Option<ProjectId>,
) -> Arc<dyn AgentSecretResolver> {
    Arc::new(TaskSecretResolver {
        server_requests: server_requests.clone(),
        store: store.clone(),
        task_id: TaskId::from(task_id.to_string()),
        project_id,
    })
}

struct TaskSecretResolver {
    server_requests: ServerRequestRuntime,
    store: Store,
    task_id: TaskId,
    project_id: Option<ProjectId>,
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
                self.resolve_secret(
                    secret_storage_key(agent_id, name),
                    format!("{agent_id} environment variable {name}"),
                )
                .map(|value| (name.clone(), value))
            })
            .collect()
    }

    fn resolve_mcp_servers(
        &self,
        capabilities: &McpCapabilities,
    ) -> Result<Vec<McpServer>, RuntimeError> {
        let project_id = match &self.project_id {
            Some(project_id) => project_id.clone(),
            None => {
                let task = self.store.read_task(self.task_id.as_str())?;
                crate::projects::task_record_project_id(&task)
            }
        };
        let definitions = self.store.effective_mcp_servers(Some(&project_id))?;
        let configured_count = definitions.len();
        let servers = project_mcp_servers(definitions, capabilities, |server_id, kind, name| {
            self.resolve_secret(
                mcp_secret_storage_key(server_id, kind, name),
                format!("MCP server {server_id} {kind} field {name}"),
            )
        })?;
        crate::logging::info(
            "mcp_servers_resolved_for_session",
            serde_json::json!({
                "taskId": self.task_id.as_str(),
                "configuredCount": configured_count,
                "projectedCount": servers.len(),
                "httpSupported": capabilities.http,
                "sseSupported": capabilities.sse,
            }),
        );
        Ok(servers)
    }
}

impl TaskSecretResolver {
    fn resolve_secret(&self, key: String, label: String) -> Result<String, RuntimeError> {
        let opened = self.server_requests.open_task_secret_read_request(
            self.task_id.clone(),
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

fn secret_storage_key(agent_id: &str, name: &str) -> String {
    format!("openaide.agent.{agent_id}.env.{name}")
}

fn mcp_secret_storage_key(server_id: &str, kind: &str, name: &str) -> String {
    format!("openaide.mcp.{server_id}.{kind}.{name}")
}

fn project_mcp_servers(
    definitions: Vec<McpServerDefinition>,
    capabilities: &McpCapabilities,
    mut resolve_secret: impl FnMut(&str, &'static str, &str) -> Result<String, RuntimeError>,
) -> Result<Vec<McpServer>, RuntimeError> {
    definitions
        .into_iter()
        .filter(|definition| definition.enabled)
        .filter_map(|definition| match definition.configuration {
            McpServerConfiguration::Stdio {
                command,
                args,
                env,
                secret_env,
                ..
            } => Some(project_stdio(
                &definition.id,
                definition.label,
                command,
                args,
                env,
                secret_env,
                &mut resolve_secret,
            )),
            McpServerConfiguration::Http {
                url,
                headers,
                secret_headers,
            } if capabilities.http => Some(project_http(
                &definition.id,
                definition.label,
                url,
                headers,
                secret_headers,
                &mut resolve_secret,
            )),
            McpServerConfiguration::Sse {
                url,
                headers,
                secret_headers,
            } if capabilities.sse => Some(project_sse(
                &definition.id,
                definition.label,
                url,
                headers,
                secret_headers,
                &mut resolve_secret,
            )),
            McpServerConfiguration::Http { .. } | McpServerConfiguration::Sse { .. } => None,
        })
        .collect()
}

fn project_stdio(
    id: &str,
    label: String,
    command: String,
    args: Vec<String>,
    mut env: BTreeMap<String, String>,
    secret_env: Vec<String>,
    resolve_secret: &mut impl FnMut(&str, &'static str, &str) -> Result<String, RuntimeError>,
) -> Result<McpServer, RuntimeError> {
    for name in secret_env {
        env.insert(name.clone(), resolve_secret(id, "env", &name)?);
    }
    Ok(McpServer::Stdio(
        McpServerStdio::new(label, PathBuf::from(command))
            .args(args)
            .env(
                env.into_iter()
                    .map(|(name, value)| EnvVariable::new(name, value))
                    .collect(),
            ),
    ))
}

fn project_http(
    id: &str,
    label: String,
    url: String,
    headers: BTreeMap<String, String>,
    secret_headers: Vec<String>,
    resolve_secret: &mut impl FnMut(&str, &'static str, &str) -> Result<String, RuntimeError>,
) -> Result<McpServer, RuntimeError> {
    Ok(McpServer::Http(McpServerHttp::new(label, url).headers(
        project_headers(id, headers, secret_headers, resolve_secret)?,
    )))
}

fn project_sse(
    id: &str,
    label: String,
    url: String,
    headers: BTreeMap<String, String>,
    secret_headers: Vec<String>,
    resolve_secret: &mut impl FnMut(&str, &'static str, &str) -> Result<String, RuntimeError>,
) -> Result<McpServer, RuntimeError> {
    Ok(McpServer::Sse(McpServerSse::new(label, url).headers(
        project_headers(id, headers, secret_headers, resolve_secret)?,
    )))
}

fn project_headers(
    id: &str,
    mut headers: BTreeMap<String, String>,
    secret_headers: Vec<String>,
    resolve_secret: &mut impl FnMut(&str, &'static str, &str) -> Result<String, RuntimeError>,
) -> Result<Vec<HttpHeader>, RuntimeError> {
    for name in secret_headers {
        headers.insert(name.clone(), resolve_secret(id, "header", &name)?);
    }
    Ok(headers
        .into_iter()
        .map(|(name, value)| HttpHeader::new(name, value))
        .collect())
}

#[cfg(test)]
#[path = "secret_resolver_tests.rs"]
mod tests;
