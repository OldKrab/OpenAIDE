# ACP Auth Secret Storage

OpenAIDE will store user-entered ACP environment-variable authentication secrets in VS Code SecretStorage, not in settings, task records, chat history, logs, or support export data. When an Agent requires an `env_var` auth method, the extension host injects the selected secret values into that Agent process environment and the runtime calls ACP `authenticate`; this supports real agents without making credentials part of the persisted OpenAIDE domain state.

Custom Agent launch environment follows the same rule for sensitive values. Settings may store plain environment values for non-sensitive configuration and secret environment variable names for credentials, but not secret values. Secret values live in VS Code SecretStorage keyed by Custom Agent identity and environment variable name. Because the runtime owns ACP Agent process launch, it requests those named secret values from the Host immediately before starting the Agent process.

Agent settings renders auth actions from ACP `authMethods`, not from Codex-specific branches. Agent-handled auth, `env_var` auth, and terminal auth are separate method types with generic UI and Host behavior.

For `env_var` auth, OpenAIDE restarts the Agent process with the stored secret values injected into the process environment, then calls ACP `authenticate` with the selected method id. Secrets are not sent through normal settings, task state, chat state, logs, support export data, or webview state.

For Custom Agent launch environment, OpenAIDE persists plain values and secret variable names in normal settings. The Settings UI may collect or replace secret values, but it must not echo them back into webview state after storage, include them in support export data, or serialize them into the runtime Agent catalog. The runtime Agent catalog carries plain values and requested secret variable names; the Host Capability RPC supplies secret values on demand.

If a Custom Agent declares a secret environment variable name but no secret value is stored for it, OpenAIDE shows Agent Status as setup required. Missing secret values are recoverable setup gaps, not failed Tasks or disabled Agents.

When a Custom Agent is deleted, OpenAIDE deletes the SecretStorage entries for that Custom Agent's secret environment variables. Existing Task history remains, but deleted Custom Agent credentials are not retained for undo or recreation.

For terminal auth, OpenAIDE opens a Host terminal using the same Agent command plus the auth method's additional args/env. OpenAIDE requires the user to confirm the setup is complete before reconnecting and retrying authentication, because terminal login completion is not reliably inferable from process launch alone.

For agent-handled auth, OpenAIDE calls ACP `authenticate`, shows auth-in-progress Agent Status, and relies on the Agent to complete the flow or return a safe error. OpenAIDE owns the status, retry, and failure display, not the internals of the Agent's auth flow.
