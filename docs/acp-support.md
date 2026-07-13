# ACP Support Matrix

OpenAIDE aims to support the full Agent Client Protocol surface, including stable protocol pages and RFD-defined draft surfaces. Stable ACP v1 support is a release target, not an optional partial-support claim. Each feature is still capability-checked against the selected Agent at runtime; unsupported Agent capabilities must degrade visibly instead of being treated as working.

This document tracks three things:

- **ACP surface**: stable protocol method, notification, content type, or RFD feature.
- **OpenAIDE support**: not started, partial, supported, or deferred with reason.
- **Agent support**: discovered from `initialize` capabilities, advertised auth/config metadata, registry metadata, or observed method behavior.

## Stable Protocol

| Surface | OpenAIDE support | Agent support source | Notes |
| --- | --- | --- | --- |
| `initialize` | partial | ACP response | Runtime initializes stdio Agents such as Codex and OpenCode, validates ACP v1, records safe capability/auth summaries for Agent Settings, and advertises workspace-scoped fs/terminal capabilities only when the VS Code Host bridge is available. |
| `authenticate` | partial | `authMethods` | Agent-handled auth methods can be invoked from Agent Settings through ACP `authenticate`. `env_var` and terminal-style auth remain planned. |
| `session/new` | partial | baseline ACP | Creates native Agent sessions for Tasks. Task preparation, config options, command discovery, and capability setup are exposed through Task snapshots and events while setup continues. |
| `session/load` | partial | `agentCapabilities.loadSession` | Runtime can adopt an external Native Session into a Task, capture replayed `session/update` history before the response, store normalized Chat history, and keep the loaded session active for follow-up prompts. Task navigation can start adoption from discovered external sessions. |
| `session/resume` | planned | `agentCapabilities.sessionCapabilities.resume` | Reconnects without replay. |
| `session/list` | partial | `agentCapabilities.sessionCapabilities.list` | Runtime exposes workspace-scoped Native Session discovery through `agent.listSessions`; task navigation requests candidates for the selected workspace and presents them in the shared Sessions list. |
| `session/close` | partial | `agentCapabilities.sessionCapabilities.close` | Frees active Agent session resources when the Agent advertises close support. |
| `session/prompt` | partial | baseline ACP | Sends text plus path attachments as resource links, and streams agent text, thoughts, tool activity, and permission requests into durable Chat state. Follow-up prompts are dispatched immediately on the same session while an earlier prompt request remains active; responses are tracked independently. Because ACP v1 updates are session-scoped rather than prompt-scoped, the newest prompt projection owns subsequent uncorrelated updates. |
| `session/cancel` | partial | baseline ACP | Sends prompt cancel and preserves Task history; bounded watchdog hardening remains. |
| `session/update` | partial | baseline ACP | Normalized before frontend; raw ACP payloads are not UI contract. Developer-only ACP tracing can persist raw stdio lines before normalization for investigation. Agent text chunks are durably committed before typed Chat append/chunk events are pushed to Frontend. Non-text updates finalize the current text run before their own event, preserving visible order. Frontend keeps authoritative received text separate from disposable frame-paced presentation state; history and recovery snapshots never replay presentation motion. Tool calls keep stable identity across create/update and carry normalized detail data for expanded Chat inspection. |
| `session/request_permission` | partial | baseline client method | Creates visible permission state and routes one selected/cancelled outcome back. |
| `session/set_config_option` | partial | session config options | Task Native Session config drives Task snapshot option selectors. Each returned option list is treated as complete render state, and option changes are reconciled through Task snapshots and events. |
| `session/set_mode` | planned | legacy modes | Compatibility fallback when config options are absent. |
| `session/set_model` | planned | legacy models where advertised | Compatibility fallback only. |
| `fs/read_text_file` | partial | client capability | Active ACP sessions can read absolute paths under open VS Code workspace roots through `openTextDocument`, so unsaved editor text is respected. |
| `fs/write_text_file` | partial | client capability | Active ACP sessions can write absolute paths under open VS Code workspace roots through `WorkspaceEdit`; missing files are created before editing. |
| `terminal/create` | partial | client capability | Active ACP sessions can start session-owned Host processes in an open workspace root with bounded output retention. |
| `terminal/output` | partial | client capability | Returns retained combined stdout/stderr plus truncation and exit status when available. |
| `terminal/wait_for_exit` | partial | client capability | Waits without a fixed HostBridge deadline and can be cancelled with the active prompt. |
| `terminal/kill` | partial | client capability | Terminates a running Host-owned terminal without releasing retained output; stubborn processes are escalated while the terminal id remains valid until exit/release. |
| `terminal/release` | partial | client capability | Invalidates the terminal id immediately, kills if still running, and keeps internal process tracking until exit. Embedded terminal display preservation remains planned. |
| content blocks | partial | prompt capabilities | Sends text and path attachments as baseline resource links. Embedded resources, images, and audio require capability-gated support. |
| tool calls | partial | session updates | Track create/update, status, kind, normalized summaries, locations, readable input fields, content blocks, and output fields with stable `toolCallId` identity. Lazy per-tool artifacts and linked resolved permission state remain in progress. |
| diff tool content | partial | tool content | Normalizes semantic old/new text into expanded edit details. Unified-diff styling and delete/move-specific renderers remain in progress. |
| terminal tool content | partial | tool content | Preserves terminal references and command output fields in expanded tool details. Embedded terminal replay remains in progress. |
| plan updates | planned | session updates | Advisory plan state. |
| slash commands | planned | `available_commands_update` | UI command discovery and invocation. |
| session info update | supported | session updates | Agent titles and activity timestamps update the persisted Task projection; an explicit clear supersedes provisional Prompt or Agent-owned titles. |

## RFD / Draft Surfaces

| RFD surface | OpenAIDE support | Agent support source | Notes |
| --- | --- | --- | --- |
| ACP Agent Registry | planned | registry metadata | Agent templates and install metadata. |
| Additional workspace roots | planned | RFD/capabilities | Multiple allowed roots for session lifecycle and file access. |
| Agent telemetry export | planned | RFD/capabilities | Support export data must remain redacted and user-safe. |
| Authentication method types | partial | `authMethods` | `agent` methods are actionable in Agent Settings. `env_var` SecretStorage injection and terminal-style auth are planned. |
| Boolean config option | planned | config option schema | Adds non-select config controls. |
| Configurable LLM providers | planned | config option/provider metadata | Agent-specific provider settings without hard-coded UI assumptions. |
| Deleted-file diff representation | planned | diff content | Preserve delete semantics beyond old/new fallback. |
| Elicitation | supported | client capability | Form elicitation is normalized into Task Questions and routed to connected clients that advertise question-response support; Task state subscriptions do not grant or revoke this connection capability. When none can answer, App Server returns cancellation without blocking the Task. |
| Logout method | planned | future auth metadata | Agent logout where advertised. |
| MCP-over-ACP | planned | RFD/capabilities | ACP channels for MCP transport when supported. |
| Message ID | planned | update metadata | Deduplication and stable message identity when present. |
| Meta propagation | planned | `_meta` | Preserve only where safe; never depend on opaque values for correctness. |
| Model config category | planned | config option category | Model selector classification. |
| Next edit suggestions | planned | RFD/capabilities | Editor suggestion surface when advertised. |
| Proxy chains | planned | registry/proxy metadata | Agent extension/proxy capability model. |
| Request cancellation mechanism | planned | RFD/capabilities | Request-level cancellation beyond prompt cancel. |
| Session delete | partial | session capabilities | Confirmed Task delete calls `session/delete` for active bound Native Sessions when advertised; local tombstones prevent re-adoption when native deletion is unavailable or fails. |
| Session fork | planned | session capabilities | Fork existing sessions when advertised. |
| Session usage/context status | planned | session updates | Context usage and limits in UI. |
| Streamable HTTP/WebSocket transport | planned | transport metadata | Non-stdio transports; not blocking first stdio process iteration. |
| ACP v2 proposal | planned | protocol negotiation | Do not use v2 semantics unless negotiated. |
| v2 prompt lifecycle | planned | protocol negotiation | Tracked separately from ACP v1 prompt turn. |

## Agent Compatibility Checks

For each configured Agent, OpenAIDE records a safe capability summary:

- protocol version and implementation info
- advertised auth methods
- session capabilities: list, load, resume, close, delete, fork
- prompt content capabilities
- config option schema and current values
- MCP transport capabilities
- client-method usage observed during turns: permissions, filesystem, terminal, elicitation
- transport type and supported lifecycle behavior

Capability summaries must not include prompt text, secrets, raw file content, or raw ACP payload dumps.
