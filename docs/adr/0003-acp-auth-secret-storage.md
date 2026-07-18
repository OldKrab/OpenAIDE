# ACP Auth Secret Storage

OpenAIDE will persist user-entered ACP environment-variable authentication secrets in App Shell secure storage, not in settings, task records, chat history, logs, or support export data. Authentication secret keys include Agent Identity, Authentication Method id, and variable name; non-secret authentication values are persisted as ordinary Agent auth configuration. When an Agent requires an `env_var` auth method, the App Shell injects the selected values into that Agent process environment and the runtime calls ACP `authenticate`; this supports repeatable startup without making credentials part of the persisted OpenAIDE domain state.

Custom Agent launch environment follows the same rule for sensitive values. Settings may store plain environment values for non-sensitive configuration and secret environment variable names for credentials, but not secret values. Secret values live in VS Code SecretStorage keyed by Custom Agent identity and environment variable name. Because the runtime owns ACP Agent process launch, it requests those named secret values from the Host immediately before starting the Agent process.

Agent settings renders auth actions from ACP `authMethods`, not from Codex-specific branches. Agent-handled auth, `env_var` auth, and terminal auth are separate method types with generic UI and Host behavior.

Agent Settings is the single authentication interaction surface. New Task and Task pages may render Auth Required status and an action that opens the affected Agent in Settings, but they do not duplicate method selection, credential forms, terminal-auth controls, or logout.

Authentication is one App Server-owned operation per Agent Identity. While it is in progress, every connected App Shell observes Authenticating status, no second authentication attempt may start for that Agent, and only the initiating App Shell owns method-specific interactive prompts or terminal confirmation.

Authentication has no fixed timeout. The initiating App Shell offers explicit Cancel; cancellation aborts or restarts the Agent process when necessary, rolls back staged auth configuration, and leaves the blocked lifecycle operation paused rather than reporting authentication success or replaying work.

OpenAIDE implements all ACP v1 Authentication Method types end to end and implements `logout` when the Agent advertises `agentCapabilities.auth.logout`; it does not present unsupported methods or a synthetic sign-out action.

Because ACP does not expose generic current-authentication state, Agent Settings shows Sign out whenever the connected Agent advertises logout rather than only after an OpenAIDE-observed sign-in. The action reports Agent errors without pretending OpenAIDE can independently determine whether logout was necessary.

OpenAIDE disables logout while the Agent owns any Running Task. ACP does not guarantee what logout does to active sessions, so OpenAIDE waits until those Tasks finish or are stopped rather than exposing provider-dependent disruption behind a generic sign-out action. Idle Tasks remain saved and may require authentication when reopened.

After successful logout, OpenAIDE removes persisted values belonging to the active `env_var` method and stops the idle Agent process so retained process environment cannot silently restore authentication. Credentials stored for other Authentication Methods remain untouched. To make that cleanup precise across restarts, OpenAIDE persists the last successfully invoked Authentication Method id as non-secret cleanup provenance. This id is not a preferred selection, is not presented as current authentication state, and does not prove that the Agent remains authenticated.

Advertised `authMethods` are available choices, not authentication state. OpenAIDE marks an Agent auth required only after the Agent returns ACP `auth_required` for an authentication-gated operation. OpenAIDE does not persist or preselect a preferred Authentication Method and never chooses or starts one merely because a method is present: the user explicitly selects from the Agent's advertised order, and successful authentication triggers automatic reconnect and retry when a blocked lifecycle operation exists. Background preparation and Native Session discovery may surface auth-required Agent Status, but must not open an interactive authentication flow on their own.

Advertised Authentication Methods remain actionable in Agent Settings even before an operation reports `auth_required`, allowing proactive explicit sign-in. A proactive flow has no blocked operation to retry and does not create synthetic persistent “signed in” state; OpenAIDE returns the Agent to Connected with no known auth blocker.

ACP exposes no generic current-authentication query. Successful `initialize` therefore makes the Agent Connected, and the first real authentication-gated operation may later transition it to Auth Required. OpenAIDE uses the actual Prepared Task or Task operation as that observation, never a disposable Native Session or background history request created solely to probe authentication.

Successful authentication automatically retries only ACP session lifecycle and readiness operations that returned `auth_required` before producing a result: Prepared Task session creation, session load, session resume, and explicit session listing. OpenAIDE never automatically replays `session/prompt`, steering, or another work-producing operation after authentication; those remain visible failures with explicit recovery.

For `env_var` auth, OpenAIDE restarts the Agent process with the stored secret values injected into the process environment, then calls ACP `authenticate` with the selected method id. Secrets are not sent through normal settings, task state, chat state, logs, support export data, or webview state.

New `env_var` values are staged through the App Shell's secure-storage transaction before the process restart. OpenAIDE commits secret and non-secret value changes only after ACP authentication succeeds; failure or cancellation restores the previous values. There is no per-sign-in remember toggle in the first version.

Agent Settings never receives a saved secret value. It receives only whether a value is saved: an empty field reuses that value, entered text transactionally replaces it after successful authentication, and a separate Forget action removes it. Non-secret auth values may be prefilled from ordinary Agent auth configuration.

For Custom Agent launch environment, OpenAIDE persists plain values and secret variable names in normal settings. The Settings UI may collect or replace secret values, but it must not echo them back into webview state after storage, include them in support export data, or serialize them into the runtime Agent catalog. The runtime Agent catalog carries plain values and requested secret variable names; the Host Capability RPC supplies secret values on demand.

If a Custom Agent declares a secret environment variable name but no secret value is stored for it, OpenAIDE shows Agent Status as setup required. Missing secret values are recoverable setup gaps, not failed Tasks or disabled Agents.

When a Custom Agent is deleted, OpenAIDE deletes the SecretStorage entries for that Custom Agent's secret environment variables. Existing Task history remains, but deleted Custom Agent credentials are not retained for undo or recreation.

For terminal auth, OpenAIDE sends a visible-terminal request to the initiating App Shell using the same Agent command plus the auth method's additional args/env. This auth request is distinct from the headless ACP tool-terminal bridge, and terminal auth is available only when the initiating App Shell advertises `openTerminal`. OpenAIDE requires the user to confirm the setup is complete before reconnecting and retrying authentication, because terminal login completion is not reliably inferable from process launch alone.

For agent-handled auth, OpenAIDE calls ACP `authenticate`, shows auth-in-progress Agent Status, and relies on the Agent to complete the flow or return a safe error. OpenAIDE owns the status, retry, and failure display, not the internals of the Agent's auth flow.
