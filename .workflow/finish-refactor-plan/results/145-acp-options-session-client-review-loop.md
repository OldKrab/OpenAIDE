# ACP Options Session Client Split: Review Loop

## Doomsday Review

Ran `$doomsday-review` with three subagents:

- Correctness: no findings.
- Requirements/tests: no findings.
- Code quality: no findings.

## Local Invariant Pass

Checked the async command boundary after the split:

- `AcpOptionsSessionClient` still owns only request/reply setup and
  stopped-worker/timeout error mapping.
- `run_options_session` still matches commands and executes every ACP operation.
- `AcpOptionsSessionManager` changed only by import path.
- Existing timeout strings and stopped-worker error strings are unchanged.

## Verification

Passed:

- `cargo test -p openaide-runtime options_session_update -- --nocapture`
- `cargo test -p openaide-runtime options_connection_list_excludes_prepared_session -- --nocapture`
- `cargo test -p openaide-runtime options_start_failure_reports_agent_error_instead_of_closed_start_channel -- --nocapture`
- `cargo test -p openaide-runtime`
- `cargo fmt --all --check`
- `npm run check`
- `git diff --check`

## Next Step

Record final integration verification and commit the completed slice.
