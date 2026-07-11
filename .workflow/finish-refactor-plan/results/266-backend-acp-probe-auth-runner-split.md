# P244 Backend ACP Probe/Auth Runner Split

## Contract

Split ACP probe/authenticate blocking runner logic out of
`openaide-rs/app-server/src/agent/acp_runtime_kernel.rs` while preserving
`AcpRuntimeKernel` as the Agent runtime coordinator.

Move synchronous thread spawning, mpsc response channels, timeout margin
handling, ACP config lookup for probe/authenticate, host bridge cloning, and
auth-method cache recording into a focused private probe/auth runner module.

Keep option-session routing, active-session routing, shutdown close
aggregation, public timeout constants, and the runtime coordinator facade in
`AcpRuntimeKernel`.

Preserve default timeout values, test-only `probe_with_timeout`, empty
auth-method validation, probe/auth timeout error text, missing command error
behavior, auth-method cache update behavior, and existing tests. Do not change
ACP protocol flow, host capability handling, option sessions, active sessions,
storage, protocol shapes, or App Server lifecycle in this slice.

## Implementation

Implemented. `AcpRuntimeKernel` remains the Agent runtime coordinator and keeps
option-session routing, active-session routing, shutdown close aggregation, and
the public timeout constants. The new private `AcpProbeAuthRunner` owns ACP
probe/authenticate config lookup, host bridge cloning, thread spawning, blocking
response channels, timeout margin handling, empty auth method validation, and
auth-method cache recording.

## Review

`$doomsday-review` ran correctness, requirements/tests, and code-quality
subagent passes. Correctness and code quality reported no findings. The
requirements/tests pass found missing edge-case coverage for runner-specific
probe/auth behavior. Added focused tests for exact probe timeout text, blank
auth method validation before agent startup, and successful auth method cache
recording. Follow-up review found authentication timeout text was still
unpinned, so the slice added a focused authentication timeout text test.

## Verification

- `cargo fmt --all --check`
- `cargo check -p openaide-runtime`
- `cargo test -p openaide-runtime agent::acp::tests -- --nocapture`
- `cargo test -p openaide-runtime runtime_settings_patch_updates_developer_acp_trace_live -- --nocapture`
- `npm run check`
- `npm test`
- `git diff --check`
- `jq empty .workflow/finish-refactor-plan/state.json`
- production source-size scan

Final follow-up requirements review reported no findings.

## Status

Completed and ready to commit.
