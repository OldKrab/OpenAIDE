# Backend Agent Runtime Contract API Contract

Accept the Backend Agent runtime contract split.

Public API:
- Keep all current Agent runtime contract names importable from
  `crate::agent::{...}`:
  `AgentSession`, `AgentLoadedSession`, `AgentSessionStart`,
  `AgentSessionResume`, `AgentSessionLoad`, `AgentPrompt`,
  `AgentConfigOptionsRequest`, `AgentSetConfigOptionRequest`,
  `AgentProbeRequest`, `AgentAuthenticateRequest`,
  `AgentListSessionsRequest`, `AgentSessionDelete`, `TurnCancellation`,
  `AgentRuntime`, `AgentEventSink`, and `AgentSessionEventSink`.
- Keep every field name, field type, derive, constructor, helper method,
  trait method signature, default implementation, default return value, and
  error message unchanged.
- Keep `agent/mod.rs` as the public Agent module facade and do not require
  call-site import changes in this slice.

Internal module contract:
- Create `agent/runtime.rs` for the Agent runtime contract.
- Move only runtime-neutral contract types and traits into that module.
- Keep `agent/mod.rs` responsible for module declarations, visibility, and
  public re-exports.
- Do not move `agent::events`, ACP internals, mock behavior, registry behavior,
  prompt content, or tool details in this slice.
- Do not add new abstraction layers or dependency bundles.

Behavior to preserve:
- `AgentSession::new` and `AgentSession::with_config_options` behavior.
- `TurnCancellation::new`, `cancel`, and `is_cancelled` semantics.
- All `AgentRuntime` default capability-missing behavior and shutdown defaults.
- `AgentEventSink` and `AgentSessionEventSink` trait contracts.

Verification:
- `cargo fmt --all`
- `cargo check -p openaide-runtime`
- `cargo test -p openaide-runtime agent:: -- --nocapture`
- `cargo test -p openaide-runtime tasks:: -- --nocapture`
- `npm run check`
- `npm test -- --runInBand`
- `git diff --check`
- Source-size scan for changed production Agent files.

