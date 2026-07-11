# Next Slice Selection

Selected the Backend Agent runtime contract split as the next refactor slice.

Why this slice:
- `openaide-rs/app-server/src/agent/mod.rs` mixes module declarations,
  public runtime request/response structs, turn cancellation state, and runtime
  trait contracts in one module.
- The Agent runtime contract is widely imported by Task lifecycle, ACP runtime,
  mock runtime, and integration tests, so keeping the existing namespace stable
  while isolating the contract improves backend module boundaries with low
  behavioral risk.
- This slice follows the existing refactor direction: keep facades stable and
  move meaningful ownership into focused modules.

Scope:
- Move Agent runtime contract structs, `TurnCancellation`, `AgentRuntime`,
  `AgentEventSink`, and `AgentSessionEventSink` out of `agent/mod.rs`.
- Keep all existing imports from `crate::agent::{...}` working.
- Do not change ACP behavior, Task lifecycle behavior, Agent registry,
  event normalization, runtime defaults, cancellation semantics, trait default
  error text, or tests.

Primary risks:
- Breaking broad imports from `crate::agent::*` in runtime and tests.
- Accidentally changing default `AgentRuntime` method behavior or capability
  error strings while moving code.
- Creating a confusing module boundary by mixing event types or ACP-specific
  internals into the new runtime contract module.

