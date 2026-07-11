# 313 LocalHttp Endpoint Integration

## Scope

Tenth A7 implementation slice: publish the reusable LocalHttp probe endpoint
from App Server startup and make attach-or-launch reuse it through the real
`client/probe` protocol path.

## Contract

- Publish a loopback-only LocalHttp probe endpoint record in runtime storage.
- Use a process-scoped high-entropy auth token stored only in the endpoint
  record.
- Keep LocalHttp probe-only: no normal product traffic beyond `client/probe`.
- Share one `RpcGateway` between stdio and LocalHttp probe handling.
- Build endpoint record facts from the same App Server probe facts used by
  `client/probe`.
- Fail closed if endpoint publication fails so a live App Server is not
  undiscoverable while holding storage.
- Cleanup only the current process endpoint record on normal shutdown/drop.
- Provide a concrete attach-or-launch runner path using LocalHttp probing.

## Implementation

- Added `SharedRpcGateway` as the transport-neutral shared gateway handle.
- Moved endpoint publication into library-level `app_server_process`.
- Wired protocol-edge startup to publish the reusable LocalHttp probe endpoint.
- Added RAII endpoint cleanup and fail-closed startup behavior.
- Added `AttachOrLaunchRunner::run_with_local_transports`.
- Added an integration test proving a published endpoint is reused through the
  real LocalHttp `client/probe` path.
- Updated the living refactor plan for the completed integration slice and next
  A7 step.

## Review

- Ran `$doomsday-review` with three focused subagents.
- Correctness: no findings after fail-closed endpoint publication fix.
- Requirements/tests finding fixed by adding the end-to-end reuse test.
- Plan/spec finding fixed by updating `docs/refactor-plan.md`.
- Code-quality finding fixed by moving publication out of the binary and
  decoupling it from stdio.
- Contract-duplication finding fixed by deriving endpoint records from shared
  probe facts.

## Verification

- `cargo fmt --all --check`
- `cargo test -p openaide-runtime app_server_process --lib`
- `cargo test -p openaide-runtime protocol_edge --lib`
- `cargo test -p openaide-runtime app_server_client --lib`
- `cargo test -p openaide-runtime --bins`
- `git diff --check`
- Production source-size scan for touched files.
