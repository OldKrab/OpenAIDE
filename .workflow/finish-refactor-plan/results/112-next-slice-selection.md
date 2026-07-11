# Next Slice Selection: Agent Registry Catalog Split

## Selected Slice

Split Agent catalog parsing, built-in definition construction, and registry tests
out of `agent/registry.rs`.

Tentative module shape:

- `agent/registry.rs`: runtime-facing `AgentRegistry`, `AgentDefinition`,
  `AgentLaunch`, `AgentSourceKind`, public constants, and registry API.
- `agent/registry_catalog.rs`: `AgentCatalogRecord`,
  `AgentCatalogSourceKind`, catalog normalization, catalog validation, and
  catalog-to-definition construction helpers.
- `agent/registry_builtin.rs`: built-in Agent definitions and launch-policy
  construction for Codex and OpenCode.
- `agent/registry/tests.rs`: focused registry tests outside the production
  source file.

## Why This Slice

`agent/registry.rs` is one of the larger remaining Agent production files and
currently mixes several responsibilities:

- runtime lookup and validation API used by Task and ACP modules;
- built-in Agent identity and launch policy;
- custom catalog parsing and validation;
- task-create validation;
- inline tests.

This boundary matters because Agent identity, built-in launch policy, and
catalog parsing will grow as Agent settings and cleanup are designed. Keeping
runtime lookup separate from catalog/build-in construction keeps the registry
facade small and makes later Agent settings work easier to review.

## Intended Boundary

`agent/registry.rs` should keep:

- `CODEX_AGENT_ID`, `CODEX_AGENT_LABEL`, `OPENCODE_AGENT_ID`,
  `OPENCODE_AGENT_LABEL`;
- `AgentSourceKind`;
- `AgentLaunch`;
- `AgentDefinition`;
- `AgentRegistry`;
- methods used by runtime code:
  - `codex`;
  - `built_ins`;
  - `default_built_ins`;
  - `from_agent_catalog`;
  - `require`;
  - `require_acp_config`;
  - `display_name`;
  - `validate_task_create`;
  - `AgentDefinition::label`;
  - `AgentDefinition::acp_stdio_config`;
  - `AgentDefinition::options_request_key`.

New helper modules should own implementation details only. Runtime call sites
should still import `AgentRegistry` and `AgentCatalogRecord` from the same
existing paths unless the API contract explicitly changes that.

## Constraints

- No behavior changes.
- Keep built-in Agent ids, labels, launch configs, and launch override behavior
  unchanged.
- Keep catalog validation errors and field names unchanged.
- Keep disabled-catalog-record behavior unchanged.
- Keep duplicate-id behavior unchanged.
- Keep task-create validation behavior unchanged.
- Move tests to a separate Rust test module/file where practical.
- Keep production Rust source files under the 400-line limit.

## Main Risks To Grill

- Whether `AgentCatalogRecord` remains re-exported from `agent::registry` for
  current environment loading code.
- Whether built-in launch construction should live in a helper module or on
  `AgentDefinition`.
- Whether tests should remain unit tests under `agent/registry/` or move into
  broader Agent integration tests.

## Next Step

Grill and record the API contract for the Agent Registry catalog split.
