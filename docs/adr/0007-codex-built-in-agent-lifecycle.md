# Built-In Agent Lifecycle

OpenAIDE is ACP-only for the current product direction, and Codex and OpenCode are built-in Agents. Agent definitions contain identity and launch information only; auth methods come from ACP `initialize.authMethods`, and Configuration Options come from session setup and update messages.

For Codex, OpenAIDE tries `codex-acp` from `PATH`, then falls back to `npx -y @agentclientprotocol/codex-acp`. For OpenCode, OpenAIDE tries `opencode acp` from `PATH`, then falls back to `npx -y opencode-ai acp`. The Agent is selectable for new Tasks only after successful ACP initialization and satisfied auth/setup requirements. Task-specific Configuration Options are discovered from each Task's Native Session after `task/create`; if Task session setup fails, OpenAIDE shows renderable Task preparation error state instead of falling back to a mock.

OpenAIDE does not guess Configuration Option defaults. A controlled no-options state is allowed only when the Agent successfully reports no Configuration Options or the Task session exposes an explicitly recoverable no-options state.

After a user completes Agent auth or setup in settings, OpenAIDE attempts to reconnect automatically. Agent settings also exposes an explicit retry action so users can rerun connection/setup checks when auto reconnect fails or external state changes.

Built-in Agents are fixed product entries, not editable templates. Each built-in exposes enable/disable, setup/auth, probe, retry, and status actions; users who need a different command, arguments, or environment create a Custom Agent instead.

The first production transport is ACP over stdio because current built-ins use process stdio. Custom Agents in the first iteration also use stdio process transport. Other ACP transports, including HTTP/WebSocket draft transport, remain tracked planned support and do not block built-in ACP Agent support.
