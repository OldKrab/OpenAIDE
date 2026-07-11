# Agent Registry Split API Contract

## Decision

Split `agent/registry.rs` into a small runtime-facing registry facade plus
internal catalog and built-in construction modules.

This slice is a structural refactor only. It must not change Agent ids, labels,
catalog parsing behavior, launch policy, task-create validation, or current
runtime import paths.

## Module Boundary

`agent/registry.rs` remains the runtime-facing API and owns:

- public Agent id and label constants:
  - `CODEX_AGENT_ID`;
  - `CODEX_AGENT_LABEL`;
  - `OPENCODE_AGENT_ID`;
  - `OPENCODE_AGENT_LABEL`;
- `AgentSourceKind`;
- `AgentLaunch`;
- `AgentDefinition`;
- `AgentRegistry`;
- runtime methods:
  - `AgentRegistry::codex`;
  - `AgentRegistry::built_ins`;
  - `AgentRegistry::default_built_ins`;
  - `AgentRegistry::from_agent_catalog`;
  - `AgentRegistry::require`;
  - `AgentRegistry::require_acp_config`;
  - `AgentRegistry::display_name`;
  - `AgentRegistry::validate_task_create`;
  - `AgentDefinition::label`;
  - `AgentDefinition::acp_stdio_config`;
  - `AgentDefinition::options_request_key`.

`agent/registry_catalog.rs` owns catalog input and conversion details:

- `AgentCatalogRecord`;
- `AgentCatalogSourceKind`;
- catalog defaults for enabled records and stdio transport;
- Agent id normalization;
- label fallback and truncation;
- catalog validation errors and field names;
- disabled-record skipping;
- catalog record to `AgentDefinition` conversion helper.

`agent/registry_builtin.rs` owns built-in Agent construction:

- Codex built-in definition construction;
- OpenCode built-in definition construction;
- built-in catalog override launch policy for known built-in ids;
- any helper that maps a known built-in id to its built-in `AcpAgentConfig`.

`agent/registry/tests.rs` owns the focused registry unit tests. The test module
is loaded from `registry.rs` with `#[cfg(test)] mod tests;` so production source
stays small and tests remain close to the registry facade.

## API Compatibility

Existing runtime call sites continue to import:

```rust
use crate::agent::registry::AgentRegistry;
use crate::agent::registry::AgentCatalogRecord;
```

`AgentCatalogRecord` is re-exported from `agent::registry` even though its
implementation moves to `agent::registry_catalog`. Other catalog internals are
not part of the runtime facade unless tests need crate-visible constructors or
types.

The new helper modules are `pub(crate)` modules under `agent/mod.rs`, but their
types and functions should be scoped as narrowly as Rust visibility allows.
Runtime modules outside Agent Registry should not import built-in or catalog
helper functions directly.

## Behavior That Must Stay Unchanged

- `AgentRegistry::codex(config)` returns a registry with only the Codex built-in
  Agent using the supplied `config`.
- `AgentRegistry::built_ins()` and `default_built_ins()` return Codex and
  OpenCode built-ins with the same ids, labels, launch commands, args, env, and
  secret env as before.
- Catalog records with `enabled = false` are skipped before validation and do
  not create agents.
- Empty resulting catalogs still return `RuntimeError::InvalidParams("agents")`.
- Duplicate enabled normalized ids still return
  `RuntimeError::InvalidParams("agents.id")`.
- Invalid ids still return `RuntimeError::InvalidParams("agents.id")`.
- Non-stdio transport or empty command still returns
  `RuntimeError::InvalidParams(format!("agents.{id}.command"))`.
- Built-in catalog records for Codex and OpenCode keep using the built-in
  launch policy instead of catalog command, args, env, or secret env.
- Custom catalog records keep using the catalog command, args, env, and
  secret env.
- Labels still trim whitespace, fall back to the original record id when empty,
  and truncate to 80 chars.
- `display_name` still prefers a non-empty selected label and truncates it to
  80 chars; otherwise it uses the registry label.
- `validate_task_create` still rejects unknown Agents and rejects `model_id`
  with `CapabilityMissing("agent_config_options")`.

## Test Expectations

Keep or add focused tests for:

- Codex built-in resolution and options request key.
- OpenCode built-in resolution and options request key.
- unknown Agent rejection.
- custom stdio catalog records.
- disabled catalog records.
- Codex built-in catalog override policy.
- OpenCode built-in catalog override policy.

The implementation should run at least:

- `cargo test -p openaide-runtime agent::registry -- --nocapture`;
- `cargo test -p openaide-runtime`;
- `cargo fmt --all --check`;
- `git diff --check`.

## Rejected Directions

- Do not make environment loading depend on `registry_catalog` directly.
- Do not expose catalog conversion helpers as a new cross-module API.
- Do not merge built-in launch construction into ACP config loading.
- Do not introduce legacy compatibility paths beyond preserving the current
  `agent::registry` imports.

## Next Step

Implement this split, then run doomsday-review with subagents against the
registry boundary, behavior preservation, and module isolation.
