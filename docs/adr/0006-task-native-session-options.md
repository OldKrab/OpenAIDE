# Task Native Session Options

OpenAIDE discovers Configuration Options from the Task's real Native Session. Opening a new Task creates a durable OpenAIDE Task first, starts ACP `session/new` for that Task, and returns a renderable `TaskSnapshot` with explicit preparation state instead of blocking the UI until setup finishes.

`task/create` does not accept Configuration Option values. ACP `session/new` uses the selected Project Context as `cwd`, the Task's allowed roots when supported, and the resolved MCP server list. The Agent returns default `configOptions` during session setup when it has any.

Configuration Options are live Agent/session state, not durable authoritative OpenAIDE Task state. On setup, load, resume, `session/set_config_option`, and `config_option_update`, OpenAIDE treats the returned `configOptions` array as the complete current option state for that Native Session and replaces the local render state rather than patching one field.

If Agent session setup, option discovery, command discovery, or authentication is still running, Task snapshots expose loading, preparing, stale, unavailable, blocked, or failed state. Frontend renders those states immediately and disables send when required state is not ready; it must not guess defaults or freeze the new Task page.

User-initiated Configuration Option changes use a Task-scoped App Server Protocol method such as `task/setConfigOption`. OpenAIDE forwards the change to ACP `session/set_config_option`, marks the UI change as pending presentation, and reconciles from the complete option catalog returned by the Agent or from a later `config_option_update`.

OpenAIDE allows user-initiated Configuration Option changes while a Task turn is running because ACP explicitly allows it. UI must show pending, confirmed, or failed state and must not imply that an already-running turn used a new value unless Backend or Agent state proves it.

After reload or App Server restart, OpenAIDE refreshes Configuration Options from the Agent through `session/load` or `session/resume` when supported. Any last-known option render data is stale until refreshed and must not become the source of truth.
