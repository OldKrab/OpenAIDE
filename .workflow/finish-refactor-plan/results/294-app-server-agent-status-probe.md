# App Server Agent Status Probe

Implemented the next A6 Agent ownership slice.

## Implementation

- Added typed App Server Protocol records for `agent/probe`.
- Added `AgentCollectionUpdated` events and Agent subscription fanout.
- Added App Server-owned `AgentStatusCache`.
- Added `AgentProductApi` so typed `agent/probe` validates Agent identity, probes through the Agent runtime, updates cached status, returns a renderable Agent collection snapshot, and publishes committed Agent collection events.
- Moved `AgentGateway` from Task internals into the Agent module so Task workflows and Agent product APIs share the Agent boundary instead of Agent code depending on Task internals.
- Added typed runtime status errors for auth-required, setup-required, and unsupported Agent states.
- Added typed probe capabilities for App Server projection while keeping legacy capability labels for existing Settings UI.
- Centralized protocol-edge event delivery conversion in `protocol_edge/messages.rs`.
- Regenerated TypeScript App Server Protocol bindings.

## Review Loop

- First `$doomsday-review` subagent pass found:
  - generic internal probe failures were being converted into successful `agent/probe` responses;
  - App Server Agent projection inferred status/capabilities from display strings;
  - protocol-edge event delivery conversion was duplicated.
- Fixed all findings.
- Second correctness and code-quality subagent passes returned no findings.
- Requirements/tests subagent pass returned no findings.

## Verification

- `cargo test -p openaide-app-server-protocol -- --nocapture`
- `cargo check -p openaide-runtime`
- `cargo test -p openaide-runtime agent::status_cache::tests -- --nocapture`
- `cargo test -p openaide-runtime agent::product_api::tests -- --nocapture`
- `cargo test -p openaide-runtime tasks::agent_service::tests -- --nocapture`
- `cargo test -p openaide-runtime snapshots::agent_collection::tests -- --nocapture`
- `cargo test -p openaide-runtime protocol_edge::stdio::tests::agent_probe_updates_agent_snapshot_and_emits_event -- --nocapture`
- `cargo test -p openaide-runtime state_sync::tests::agent_collection_update_delivers_to_agent_subscribers -- --nocapture`
- `cargo test -p openaide-runtime agent::acp_errors::tests -- --nocapture`
- `cargo test -p openaide-runtime agent::acp::tests::initialize_protocol_rejects_unsupported_major_version -- --nocapture`
- `npm run protocol:check`
- `npm run build --workspace @openaide/app-server-client`
- `git diff --check`

## Remaining A6 Work

- App Server-owned custom Agent mutation/settings storage.
- Project canonicalization ownership.
- Runtime restart/update policy cleanup.
