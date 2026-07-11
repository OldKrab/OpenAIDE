# P400 Custom Agent Launch Replacement Implementation

## Result

- Added `AgentReplaceCustomCleanup`.
- Added `AgentReplaceCustomHistoryPolicy` with `preserveHistoricalTasks`.
- Extended `AgentReplaceCustomResult` with typed cleanup metadata.
- Made `AgentStatusCache::clear` return whether a cached status was actually removed.
- Returned honest cleanup facts from Backend replacement:
  - old catalog record removed
  - old cached status removed when present
  - settings overlay not removed when no overlay storage exists
  - historical Tasks preserved
- Regenerated TypeScript App Server Protocol bindings.
- Added Rust and Frontend coverage for the replacement cleanup contract.

## Verification

- `cargo fmt --all`
- `cargo check -p openaide-runtime -p openaide-app-server-protocol`
- `cargo test -p openaide-runtime agent::product_api::tests::custom_agent_replacement_reports_cleanup_and_preserves_history_policy`
- `cargo test -p openaide-runtime agent::status_cache::tests::clear_removes_cached_status_and_capabilities`
- `cargo test -p openaide-runtime protocol_edge::stdio::tests::agent_custom_update_and_replace_use_distinct_identity_rules`
- `cargo test -p openaide-app-server-protocol generated_bindings_include_protocol_method_maps`
- `npm run protocol:generate`
- `npm run protocol:check`
- `npm run build --workspace @openaide/app-server-client`
- `npm run check --workspace openaide-frontend`
- `npm run check --workspace openaide-vscode-extension`
- `npm run test --workspace openaide-frontend -- appControllerCallbacks.test.ts`
