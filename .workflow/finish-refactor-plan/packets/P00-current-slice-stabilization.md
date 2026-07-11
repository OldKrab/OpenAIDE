# P00-current-slice-stabilization

## Objective

Stabilize and commit the currently uncommitted doomsday-review fixes before starting any
new refactor slice.

## Context

The worktree contains fixes for the latest review findings:

- client lifecycle reattach replacement,
- client-scoped state event filtering,
- unified initialize/state-stream cursor ownership,
- `serverStopping` protocol error,
- unsubscribe and lifecycle regression tests,
- root test wiring for `@openaide/app-server-client`,
- `@openaide/app-shell-contracts` for transitional shell/webview contracts.

## Files / Sources

- `openaide-rs/app-server-protocol/src/errors.rs`
- `openaide-rs/app-server/src/app_lifecycle.rs`
- `openaide-rs/app-server/src/client_lifecycle.rs`
- `openaide-rs/app-server/src/state_sync.rs`
- `openaide-rs/app-server/src/protocol_edge.rs`
- `packages/app-shell-contracts/`
- `packages/frontend/`
- `apps/vscode-extension/`
- root package scripts and lockfile

## Ownership

Current changed files only. Do not start the next slice in this packet.

## Do

- Inspect the diff for accidental scope creep.
- Confirm `packages/app-server-client` remains App Server Protocol focused.
- Confirm `packages/app-shell-contracts` is treated as shell/webview transitional contract
  ownership, not a new product seam.
- Run root validation.
- Commit with a neutral project-facing message.

## Do Not

- Delete legacy UI or runtime code in this packet.
- Rename Cargo crates or packages in this packet.
- Start designing the next API slice before committing this baseline.

## Expected Output

- Clean committed baseline.
- Result note under `.workflow/finish-refactor-plan/results/`.

## Verification

- `cargo fmt --all`
- `npm run protocol:check`
- `npm run check`
- `npm test`
- `npm run build:frontend`
- source-size scan for production files near 300/400 lines
