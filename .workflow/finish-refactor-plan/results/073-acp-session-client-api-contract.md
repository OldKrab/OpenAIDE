# P52 ACP Session Client API Contract

Completed: 2026-06-27T03:39:17+03:00

## Accepted Shape

Add a focused internal module:

- `agent/acp_session_client.rs`

It owns the channel-facing interface used by `AcpRuntimeKernel` to communicate with
the background ACP session worker:

- `AcpSessionClient`
- `AcpSessionCommand`
- `AcpSessionWorkerInput`
- `AcpSessionOpenRequest`
- `AcpStartedSession`
- `record_terminal_error`

`agent/acp_session_worker.rs` keeps the actual async worker entry point:

- `run_acp_session(input)`
- ACP `Client` builder and request handlers;
- session start/load setup;
- prompt command handling through `acp_prompt_runner`;
- active session update reading;
- config catalog buffering and delivery.

## Stable API

The extracted module must preserve existing internal call shapes used by
`AcpRuntimeKernel` and ACP tests:

- `AcpSessionClient::new(command_tx, cancel_tx, close_tx, terminal_error)`
- `AcpSessionClient::set_event_sink(sink)`
- `AcpSessionClient::prompt(prompt, sink)`
- `AcpSessionClient::cancel()`
- `AcpSessionClient::close()`
- `AcpSessionClient::delete()`
- `AcpSessionOpenRequest::agent_id()`
- `AcpSessionOpenRequest::task_id()`
- `AcpSessionOpenRequest::operation_name()`
- `record_terminal_error(terminal_error, error)`

`worker_stopped_error`, readable error normalization, and runtime-error-prefix
stripping remain implementation details of the new client module.

## Ownership

- `acp_session_client.rs` owns the synchronous caller-facing session handle,
  command envelopes, worker input/startup result structs, and terminal-error
  presentation for stopped workers.
- `acp_session_worker.rs` owns live ACP I/O and the command loop that interprets
  `AcpSessionCommand`.
- `acp_runtime_kernel.rs` owns the session registry, thread spawning, duplicate
  active-session protection, and runtime-level session lifecycle entry points.

## Non-Goals

- No ACP behavior change.
- No command channel semantics change.
- No timeout value change for prompt, close, or delete paths.
- No worker-stopped error text change.
- No session start/load/close/delete request ordering change.
- No config catalog buffering or delivery behavior change.
- No public Agent runtime API change.
- No test deletion or weakening.

## Review And Test Requirements

- Existing ACP session worker tests for stopped-worker error text must move with the
  error helper or remain equally focused.
- Existing ACP runtime tests that import session config catalog helpers must keep
  passing.
- `cargo test -p openaide-runtime agent::acp_session_client::tests -- --nocapture`
  should pass when the moved tests live in the new module.
- `cargo test -p openaide-runtime agent::acp::tests -- --nocapture` must pass.
- `cargo test -p openaide-runtime`, `npm run check`, and `npm test` must pass before
  commit.
- Touched production source files must stay below the source-size limit where possible;
  `acp_runtime_kernel.rs` remains a documented later split target if still oversized.
