# Agent Registry Split Implementation

## Summary

Implemented the accepted Agent Registry split as a structural refactor with no
intentional behavior changes.

## Code Changes

- Added `agent/registry_builtin.rs` for Codex and OpenCode built-in
  `AgentDefinition` construction and known built-in launch override lookup.
- Added `agent/registry_catalog.rs` for `AgentCatalogRecord`,
  `AgentCatalogSourceKind`, catalog defaults, catalog validation, id
  normalization, label normalization, disabled-record skipping, and
  catalog-to-definition conversion.
- Moved focused registry tests from inline `registry.rs` test module to
  `agent/registry/tests.rs`.
- Kept `agent/registry.rs` as the runtime-facing facade for:
  - Agent constants;
  - `AgentSourceKind`;
  - `AgentLaunch`;
  - `AgentDefinition`;
  - `AgentRegistry`;
  - runtime lookup and validation methods.
- Preserved the existing `crate::agent::registry::AgentCatalogRecord` import
  path through a facade re-export.

## Behavior Preservation

The implementation preserves:

- built-in Codex and OpenCode ids, labels, launch commands, args, env, and
  secret env behavior;
- `AgentRegistry::codex(config)` one-Agent registry behavior;
- disabled catalog records being skipped before validation;
- empty catalog, invalid id, duplicate id, and invalid command error fields;
- known built-in catalog launch override policy;
- custom catalog command, args, env, and secret env behavior;
- label trim, fallback, and 80-character truncation;
- `display_name` selected-label precedence and truncation;
- task-create validation for unknown Agents and unsupported `model_id`.

## File Size Check

Production Rust files after the split:

- `agent/registry.rs`: 156 lines;
- `agent/registry_builtin.rs`: 38 lines;
- `agent/registry_catalog.rs`: 121 lines.

All are below the 400-line production source file limit.

## Next Step

Complete the doomsday-review loop and integration verification, then commit the
slice.
