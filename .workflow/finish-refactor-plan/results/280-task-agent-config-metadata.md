# A3i Task Agent Config Metadata

## Contract

Preserve and project Agent-provided config option metadata instead of reducing
Task Agent config to opaque key/value pairs.

- Store the full Agent `ConfigOptionsCatalog` returned by session setup.
- Project option labels, descriptions, semantic categories, current values, and
  allowed values into `TaskAgentConfigSnapshot`.
- Keep the stored catalog current when App Server accepts an idle
  `task/setConfigOption`.
- Persist Agent-emitted config catalog updates from live session events.
- Preserve legacy/recovery behavior for records that have only key/value config
  state.

## Status

Completed.

## Implementation

- Added optional durable `config_options_catalog` to Task records and internal
  Task snapshots.
- Carried full config catalog through `AgentSession`.
- Stored prepared/start-session catalog metadata on Task readiness.
- Projected catalog metadata through `snapshots::task_snapshot::readiness`.
- Added unsupported fallback projection for stored config keys missing from a
  catalog.
- Updated `task/setConfigOption` to refresh the stored catalog current value.
- Updated live Agent config events to persist the full catalog.

## Review

- Initial review found live Agent config events did not persist the full catalog
  and catalog-backed projection could hide stored config keys absent from the
  catalog.
- Fixes persist live catalogs and add fallback projection for catalog-missing
  stored keys.
- Focused re-review found model-category config changes left `model_id` stale;
  the mutation now recomputes `model_id` from the updated catalog.

## Verification

- `cargo fmt --all`
- `cargo check -p openaide-runtime`
- `cargo test -p openaide-runtime tasks::product_api -- --nocapture`
- `cargo test -p openaide-runtime protocol_edge -- --nocapture`
- `cargo test -p openaide-runtime active_config_option_updates_mutate_task_settings_summary -- --nocapture`
- `cargo test --workspace -- --test-threads=1`
- `cargo fmt --all --check`
- `npm run check`
- `npm run test --workspace @openaide/app-server-client`
- `git diff --check`
- `jq empty .workflow/finish-refactor-plan/state.json`

## Next

Commit this sub-slice, then continue A3 with slash-command readiness.
