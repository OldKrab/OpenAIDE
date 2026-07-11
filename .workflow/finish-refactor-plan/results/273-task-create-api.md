# A3b Task Create API

## Contract

Implement `task/create` as the first mutating App Server Protocol Task method.

- Accept typed `projectId` and `agentId`; do not add raw workspace path fields to
  the public Task API.
- Resolve Project context through the Backend-owned Project resolver.
- Validate Agent identity through the Backend Agent registry.
- Persist a durable idle Task with no user message, no active turn, no Agent
  prompt, and `firstPromptSent=false`.
- Return a renderable Task snapshot.
- Publish Task Navigation state through generic state sync after durable
  acceptance.
- Keep `task/send`, `task/setConfigOption`, `task/cancel`, and `task/discard`
  unsupported until their workflow contracts are implemented.

## Status

Completed.

## Implementation

- Added `tasks::product_api::TaskProductApi` and `TaskCreateWorkflow`.
- Wired `task/create` into `protocol_edge::RpcGateway`.
- Wired stdio composition to use environment-backed Agent registry,
  `StorageProjectResolver`, and `TaskProductApi`.
- Added product API tests for successful create, unknown Project, and unknown
  Agent.
- Added protocol-edge stdio test proving create persists an idle Task without a
  prompt or active turn.
- Fixed state-sync publication so Project-filtered Task Navigation subscribers
  receive created Task updates through `TaskUpdated`.

## Review

Round 1 found one accepted correctness issue: `task/create` published only a
full Task Navigation snapshot, which project-filtered Task Navigation
subscriptions intentionally ignore. Fixed by publishing `TaskUpdated` for the
created Task before the full navigation snapshot and adding a regression test.

## Verification

- `cargo fmt --all`
- `cargo fmt --all --check`
- `cargo check -p openaide-runtime`
- `cargo test -p openaide-runtime tasks::product_api -- --nocapture`
- `cargo test -p openaide-runtime protocol_edge::stdio::tests::task_create_persists_idle_task_without_prompt_after_initialize -- --nocapture`
- `cargo test -p openaide-runtime state_sync::tests::task_updated_delivers_to_project_filtered_task_navigation_subscribers -- --nocapture`
- `cargo test -p openaide-runtime protocol_edge -- --nocapture`
- `cargo test --workspace -- --test-threads=1`
- `npm run check`
- `npm run test --workspace @openaide/app-server-client`
- `git diff --check`
- `jq empty .workflow/finish-refactor-plan/state.json`

## Next

Commit this sub-slice, then implement `task/send`.
