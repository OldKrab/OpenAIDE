# App Server Agent Collection Snapshots

Status: implemented, reviewed, verified, and committed-ready.

## Scope

- Added App Server Agent summary projection from `AgentRegistry`.
- Added an Agent collection snapshot source seam under `snapshots`.
- Included Backend-owned Agent collections in `client/initialize` snapshots and `state/subscribe` Agent snapshots.
- Wired the stdio gateway factory to use the App Server Agent registry as the Agent snapshot source.
- Mapped `ClientSnapshot.agents` into Frontend Agent options during initialize.
- Preserved explicit user Agent selection when initialize resolves late.

## Deliberate Limits

- Agent status was projected as `disconnected` in this slice. The next slice, `294-app-server-agent-status-probe.md`, adds App Server-owned status/probe updates through typed `agent/probe`.
- Frontend icon/description values remain presentation decoration derived locally when protocol summaries do not carry those fields.
- Custom Agent mutation and secret-backed settings are not migrated in this slice.

## Verification

- `cargo test -p openaide-runtime snapshots::tests -- --nocapture`
- `cargo test -p openaide-runtime protocol_edge::stdio::tests::initialize_succeeds_through_protocol_edge_stdio -- --nocapture`
- `cargo test -p openaide-runtime agent::registry::tests -- --nocapture`
- `cargo check -p openaide-runtime`
- `npm run test --workspace openaide-frontend -- appServerAgents.test.ts appController.test.tsx`
- `npm run check --workspace openaide-frontend`
- `npm run test --workspace openaide-frontend`
- `git diff --check`
- Subagent review after fixes: no findings.
