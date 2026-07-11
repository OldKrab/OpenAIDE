# ACP Session Worker Wiring Integration Verification

Verification run for the ACP session worker client/host wiring split.

## Checks

- `cargo fmt --all`
- `cargo test -p openaide-runtime agent::acp_session_connection::tests -- --nocapture`
- `cargo test -p openaide-runtime agent::acp::tests -- --nocapture`
- `cargo check -p openaide-runtime`
- `cargo test -p openaide-runtime`
- `npm run check`
- `npm test`
- `git diff --check`
- Source-size scan for changed Rust source files:
  - `agent/acp_session_connection.rs`: 159 lines
  - `agent/acp_session_worker.rs`: 302 lines
  - `agent/acp_session_connection/tests.rs`: test file, exempt from production source-size limit

## Result

All listed checks passed.

The first `npm test` attempt hit a timeout in the existing
`duplicate_active_session_id_closes_losing_worker` runtime test after that same
test had passed in `cargo test -p openaide-runtime`. The targeted test rerun
passed, and the full `npm test` rerun passed.
