# P00 Current Slice Stabilization

Completed: 2026-06-26T18:58:51+03:00

## Scope

Stabilized the current App Server skeleton slice after review findings. This result covers
only the uncommitted fixes for the first App Server implementation slice and workflow
bootstrap artifacts.

## Fixed Findings

- Replaced stale client connection mappings when a client reinitializes with the same
  `clientInstanceId`, so closing an old transport cannot expire an active reattached
  client.
- Restricted client-scoped state events to the matching client subscription.
- Moved initialize snapshots onto the `StateStream` cursor lineage.
- Added structured `serverStopping` protocol error support.
- Added missing active unsubscribe, stopping initialize, cursor-ordering, client-scoped
  delivery, and duplicate-connection lifecycle regression tests.
- Kept current shell/webview bridge contracts out of `@openaide/app-server-client` by
  adding `@openaide/app-shell-contracts`.
- Wired App Server client package tests into the root `npm test` path.

## Verification

- `cargo fmt --all`
- `npm run protocol:generate`
- `npm run protocol:check`
- `npm run check`
- `npm test`
- `npm run build:frontend`
- Source-size scan for touched production files

## Residual Risk

- `@openaide/app-shell-contracts` is a transitional shell-contract boundary for the
  current webview UI. P01 must record its ownership and planned replacement path in the
  top-level refactor plan.
- The Cargo package under `openaide-rs/app-server` is still named `openaide-runtime`.
  Renaming is outside this stabilization slice and should be considered in a planned
  Backend layout slice.

## Next

Proceed to `P01-plan-refresh`: update `docs/refactor-plan.md` with completed slices,
the shell-contract package decision, and the next API slice candidate.
