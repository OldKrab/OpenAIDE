# P02 Process Lifecycle And State-Root API Contract

Completed: 2026-06-26T19:32:07+03:00

## Selected Slice

Backend process lifecycle, shared-instance discovery, and state roots.

## Accepted Interfaces

- `app_lifecycle`: in-process lifecycle reducer for running/draining/stopping,
  initialize admission, last-client shutdown decisions, reconnect grace effects,
  graceful shutdown planning, and shutdown completion classification.
- `storage_runtime`: state-root normalization and fingerprinting, runtime/cache
  endpoint-record placement, endpoint record primitives, launch locks, storage writer
  protection, storage-open compatibility checks, and crash-recovery classification facts.
- `app-server-client`: reusable shell attach-or-launch interface for endpoint lookup,
  launch lock acquisition, stale endpoint cleanup, endpoint validation/probe, compatible
  server reuse, process launch request construction, local auth token handling, and
  closed attach-or-launch outcomes.

## Explicit Non-Goals For First Implementation

- Do not wire Web/Desktop/VS Code shell launchers yet.
- Do not implement normal App Server Protocol traffic proxying.
- Do not implement browser transport or Caddy/domain-specific behavior.
- Do not implement durable Task recovery or Native Session takeover logic.
- Do not perform broad storage migrations.
- Do not put endpoint records into durable product state.

## Required Test Obligations

- Same state root yields the same fingerprint.
- Different roots do not collide in normal cases.
- Endpoint records are runtime/cache state, not durable product state.
- Stale endpoint records are cleaned only after failed authoritative probe.
- Attach-or-launch reuses a compatible live server.
- Concurrent launch attempts elect one writer/launcher.
- Initialize during draining aborts draining.
- Initialize during stopping is rejected.
- Last-client expiry starts draining.
- Reconnect before expiry prevents shutdown.
- Storage locked by another live server returns a structured blocked outcome.
- Clean shutdown removes endpoint records.
- Unclean shutdown is classified for recovery without auto-resuming Agent work.

## Next

Proceed to `P03-implementation-slice`: implement the narrow accepted slice with focused
tests before wiring shell launchers, transports, or broad storage behavior.
