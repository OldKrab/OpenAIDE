# ACP Session Worker Wiring API Contract

Date: 2026-06-27

## Decision

Split active-session ACP client and host request wiring from
`agent/acp_session_worker.rs` into a focused module named
`agent/acp_session_connection.rs`.

This is a narrow deep-module split: one small interface hides the long ACP
`Client::builder()` handler registration chain.

## New Module Interface

`agent/acp_session_connection.rs` owns one external seam for the worker:

```rust
pub(super) struct AcpSessionConnectionContext {
    pub(super) host_bridge: HostBridge,
    pub(super) trace: Option<AcpTraceSession>,
    pub(super) current_prompt: Arc<Mutex<Option<LivePromptProjection>>>,
    pub(super) load_replay: Arc<Mutex<Option<LoadReplayCapture>>>,
}

pub(super) async fn connect_acp_session_client<R>(
    agent: impl agent_client_protocol::ConnectTo<Client>,
    context: AcpSessionConnectionContext,
    run: impl AsyncFnOnce(ConnectionTo<Agent>) -> agent_client_protocol::Result<R>,
) -> agent_client_protocol::Result<R>;
```

Exact Rust generics may be adjusted during implementation to satisfy the ACP
crate's builder types, but the module must preserve the same interface concept:
the worker provides the ACP Agent transport, the renderable wiring context, and
the session lifecycle closure; the new module wires inbound ACP messages and
invokes the closure with `ConnectionTo<Agent>`.

## Owned By `acp_session_connection.rs`

- Set the active ACP client builder name to `openaide`.
- Register `session/update` notification handling for active sessions.
- Record notification trace entries with the existing
  `"agent_to_client" / "session/update"` trace shape.
- Capture load replay updates when `LoadReplayCapture` is active and the
  notification session id matches the replay session id.
- Return unhandled notifications as `Handled::No` with `retry: false`.
- Construct `AcpHostCapabilityHandlers`.
- Register Agent-initiated host capability request handlers:
  - `session/request_permission`;
  - `fs/read_text_file`;
  - `fs/write_text_file`;
  - `terminal/create`;
  - `terminal/output`;
  - `terminal/wait_for_exit`;
  - `terminal/kill`;
  - `terminal/release`.
- Preserve all response/error mapping currently produced by the inline worker
  handler chain.

## Kept In `acp_session_worker.rs`

- `AcpSessionWorkerInput`.
- `AcpSessionOpenRequest`.
- `AcpStartedSession`.
- `run_acp_session`.
- ACP Agent process creation from `AcpAgentConfig`.
- initialize request construction and tracing.
- `initialize_agent_connection`.
- `AcpSessionRunner` construction.
- prompt attachment validation and prompt content policy.
- start/load branching, cwd normalization, config option application, load replay
  consumption, and startup error reporting.
- active prompt command loop for prompt/cancel/close/delete.
- session config catalog buffering and delivery helpers.

## Invariants

- No public runtime API changes.
- No ACP schema or generated binding changes.
- No behavior changes to initialize/start/load/prompt/cancel/close/delete.
- No behavior changes to permissions, filesystem requests, terminal requests,
  host bridge calls, cancellation observation, trace payloads, or replay capture.
- The worker remains the session lifecycle module; the new module is only the ACP
  client/host wiring module.
- The new module must not expose one helper per handler to the worker. Handler
  registration stays behind the single connection interface.

## Non-Goals

- Do not split session config catalog delivery in this slice.
- Do not refactor options sessions.
- Do not introduce new host bridge abstractions.
- Do not change test fixture behavior except where required by imports/module paths.

## Verification Plan

- `cargo fmt --all`
- `cargo check -p openaide-runtime`
- `cargo test -p openaide-runtime agent::acp::tests -- --nocapture`
- `cargo test -p openaide-runtime`
- `npm run check`
- `npm test`
- `cargo fmt --all --check`
- `git diff --check`
- production source-size scan.
