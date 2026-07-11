# Task Turn Event Sink Split: API Contract

## Accepted Contract

Split `tasks/turn_events.rs` into a small facade plus focused child modules
without changing runtime behavior.

## Caller-Facing Surface

`tasks/turn_events.rs` remains the caller-facing module for current Task
lifecycle code and keeps these imports stable:

- `TaskEventSink`;
- `TaskSessionEventSink`;
- `PermissionWaiters`;
- `PermissionWaiter` if existing internal callers still need it.

Task lifecycle code continues to construct event sinks the same way. No caller
outside the `tasks` internals should learn about streaming run state,
permission-waiter internals, or config-option commit helpers.

## Internal Module Boundaries

Create child modules under `tasks/turn_events/`:

- `streaming.rs` owns streamed text and thought run accumulation state.
  It allocates stable message ids for the current text/thought run, appends
  chunks, returns the accumulated `NormalizedMessage`, and supports clearing
  text, thought, or both runs.
- `permissions.rs` owns `PermissionWaiters`, `PermissionWaiter`, response
  resolution, cancellation-aware waiting, and registry insertion/removal helpers
  where useful.
- `config.rs` owns Task config option commit projection for both active turn
  event sinks and session-level option changes.

The facade owns event routing:

- route `ConfigOptionsChanged` to config update and clear streaming runs;
- route text chunks through streaming text accumulation;
- route thought chunks through streaming thought accumulation;
- route tool calls through identity upsert after preserving `scope_id`
  behavior;
- route permission requests through permission waiter registration, message
  append, wait, and cleanup.

## Behavior Invariants

This slice must preserve:

- event normalization output and timestamps;
- text and thought chunk coalescing semantics;
- when text/thought streaming runs are cleared;
- generated message ids for streaming runs staying stable within a run and new
  between runs;
- tool-call `scope_id` defaulting to the current `turn_id`;
- append vs upsert write modes;
- active-turn and cancellation guards before durable mutation;
- Task `unread`, `updated_at`, `last_activity`, and `Blocked` status behavior;
- config option projection to `task.config_options` and `task.model_id`;
- permission waiter cleanup on append failure, cancellation, and successful
  resolution;
- current blocking wait behavior and cancellation polling cadence.

## Out Of Scope

- No migration to `server_requests`.
- No new durable recovery semantics.
- No App Server Protocol/state stream publication changes.
- No Agent ACP event normalization changes.
- No public protocol or TypeScript binding changes.
- No broad Task lifecycle refactor beyond imports required by the split.

## Review Requirements

`$doomsday-review` must check at least:

- facade callers do not depend on child module internals;
- the split did not widen visibility beyond `pub(crate)` or `pub(super)` where
  necessary;
- permission waiter cleanup still happens on every path;
- streaming run clear rules match the old implementation;
- config updates still no-op when the active turn no longer matches or is
  cancelled.

## Verification Plan

Run focused Runtime tests for:

- streaming text/thought behavior if covered by existing tests;
- permission request response/cancellation behavior if covered by existing
  tests;
- active-turn event mutation guards.

Then run:

- `cargo test -p openaide-runtime`;
- `cargo fmt --all --check`;
- `npm run check`;
- `git diff --check`.
