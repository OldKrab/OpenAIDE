# 305 Attach-Or-Launch Runner

## Scope

Second A7 implementation slice: add the shell-neutral effectful runner around
the pure attach-or-launch decider.

## Contract

- Keep shell-specific process launching out of the runner.
- Read the runtime endpoint record for the state root.
- Probe existing endpoint records through an injected probe interface.
- Clean stale endpoint records only after the decider says to clean them.
- Acquire the launch lock only when no viable endpoint remains.
- Return a handoff action:
  - attach to a verified existing endpoint;
  - launch a new App Server while holding the launch lock;
  - wait for another launcher;
  - fail with a typed attach-or-launch failure.
- Preserve local auth material inside backend-internal target values and avoid
  logging it through `Debug`.

## Non-Goals

- No actual HTTP/websocket/stdio probing implementation.
- No shell process spawning.
- No VS Code/Web/Desktop shell integration.
- No browser transport bootstrap.

## Implementation

- Added `app_server_client::runner` with:
  - `AttachOrLaunchRunner`;
  - `EndpointProber`;
  - typed attach-or-launch requirements;
  - attach, launch, wait, and typed-failure handoff results;
  - runner errors for probe failures, stale report mismatches, repeated stale
    cleanup, and internal invariant violations.
- The runner derives the request fingerprint from `StateRootFingerprint` so
  callers cannot pass conflicting state-root identities.
- Storage-writer blockage returns a typed failure before probing or lock
  acquisition.
- Endpoint probe reports are validated against the exact target and
  requirements sent to the prober.
- Endpoint record replacement during probe causes a retry from current state,
  not attachment from stale evidence.
- Stale cleanup uses `EndpointRecordStore::remove_if` so comparison and removal
  are guarded by the endpoint-record lock.
- `EndpointRecordStore` now serializes endpoint record write, remove, and
  conditional remove through a per-record lock.
- Replaced the stale A3 active slice contract in `docs/refactor-plan.md` with
  the current A7 runner contract.

## Review

- Initial review found probe/report race handling, stale cleanup races, duplicate
  state-root identity inputs, public impossible handoffs, and stale planning
  docs.
- Fixed those issues and added regression tests.
- Re-review found two remaining issues: impossible lock/decision disagreements
  still became public handoffs, and compare-before-remove was not atomic enough.
- Fixed those by returning `InvariantViolation` for lock/decision disagreement
  and moving conditional cleanup into `EndpointRecordStore`.
- Final narrow re-review reported clean.

## Verification

- `cargo fmt --all --check`
- `cargo test -p openaide-runtime app_server_client --lib`
- `cargo test -p openaide-runtime storage_runtime --lib`
- `cargo test -p openaide-runtime`
- `jq empty .workflow/finish-refactor-plan/state.json`
- `git diff --check`
- Production Rust source-size scan under `openaide-rs/app-server/src`

## Next

Implement the real endpoint probe and local transport connection slice that can
feed `EndpointProbeReport` into this runner.
