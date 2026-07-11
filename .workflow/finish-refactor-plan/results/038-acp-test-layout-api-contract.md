# P17 ACP Test Layout API Contract

Completed: 2026-06-27T02:40:50+03:00

## Accepted Shape

`agent/acp.rs` stays the production facade for `AcpAgentRuntime`:

- construction helpers remain unchanged;
- `AgentRuntime` trait implementation remains unchanged;
- test-only helper `probe_with_timeout` remains available to the test module.

ACP runtime tests move to:

- `openaide-rs/app-server/src/agent/acp/tests.rs`

`agent/acp.rs` declares only:

- `#[cfg(test)] mod tests;`

## Ownership

- `AcpAgentRuntime` remains a thin facade over `AcpRuntimeKernel`.
- `AcpRuntimeKernel` remains the implementation owner.
- The new `agent/acp/tests.rs` module owns only tests and test helpers.

## Non-Goals

- No behavior changes.
- No test deletion or weakening.
- No Agent runtime API changes.
- No ACP protocol mapping changes.
- No module renaming beyond the test submodule path.

## Review And Test Requirements

- The moved tests must still compile and run under `cargo test -p openaide-runtime`.
- `cargo test -p openaide-runtime agent::acp::tests -- --nocapture` must pass.
- `agent/acp.rs` should become a small production file below the project source-size
  limit.
- The move must not introduce project-facing provenance comments or docs.
