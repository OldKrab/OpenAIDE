# 304 Attach-Or-Launch Decision API

## Scope

First A7 implementation slice: deepen the reusable App Server client
attach-or-launch decision API before wiring real shell launchers or transports.

## Problem

The current `app_server_client` seed returns broad outcomes such as
`ProbeRequired`, `StaleEndpointCleaned`, or `LaunchRequired`. That is not deep
enough for a real shared launcher because callers need a clear next action:
probe a specific endpoint, attach to a verified endpoint, clean a stale record,
wait for another launcher, launch a new server, or fail with a typed reason.

## Contract

- Keep this slice pure and deterministic.
- Do not spawn processes, open sockets, or integrate VS Code yet.
- The decider consumes:
  - attach-or-launch request facts;
  - optional endpoint record;
  - optional authoritative probe result;
  - launch lock state;
  - storage writer state.
- The decider returns a stepwise decision:
  - `ProbeEndpoint`;
  - `AttachExisting`;
  - `LaunchNew`;
  - `WaitForLaunch`;
  - `CleanStaleEndpoint`;
  - `Fail`.
- Stale cleanup remains probe-driven except for clearly local record mismatch or
  stopping lifecycle hints.
- Storage writer blockage wins over all launch/reuse decisions.

## Non-Goals

- No local HTTP/websocket/stdio transport implementation.
- No endpoint probing implementation.
- No VS Code process launcher integration.
- No App Server Protocol initialize changes.

## Implementation

- Replaced broad attach-or-launch outcomes with `AttachOrLaunchDecision`.
- Added self-contained endpoint targets for probe, attach, and stale-clean
  decisions.
- Bound probe reports to both endpoint target and required protocol/app
  versions, so stale probe evidence cannot attach a different endpoint.
- Split failure reasons from stale-cleanup reasons.
- Kept auth tokens in the target for local attach/probe mechanics, with a
  redacted `Debug` implementation.
- Added focused coverage for compatible attach, stale probe rejection,
  local-record cleanup, probe-reported cleanup, typed failures, launch, wait,
  and storage-writer blockage.

## Review

- Initial correctness review reported no findings.
- Requirements and module-quality review found that the first API shape was not
  self-contained enough and accepted unbound probe evidence.
- Fixed by carrying full endpoint targets and requirements in decisions and
  probe reports, carrying cleanup targets, and renaming stopping reasons by
  source.
- Re-review reported the patched shape clean.

## Verification

- `cargo fmt --all --check`
- `cargo test -p openaide-runtime app_server_client --lib`
- `cargo test -p openaide-runtime`
- `jq empty .workflow/finish-refactor-plan/state.json`
- `git diff --check`
- Production Rust source-size scan under `openaide-rs/app-server/src`

## Next

Implement the effectful attach-or-launch runner around this decider: endpoint
record discovery, probe execution, launch-lock use, stale cleanup, wait/retry,
and attach/launch handoff.
