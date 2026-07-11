# ACP Session Request I/O Split Integration Verification

Verification run for the ACP session request I/O split.

## Checks

- `cargo fmt --all`
- `cargo test -p openaide-runtime agent::acp::tests -- --nocapture`
- `cargo check -p openaide-runtime`
- `cargo test -p openaide-runtime`
- `npm run check`
- `npm test`
- `git diff --check`
- Source-size scan for changed Rust source files:
  - `agent/acp_session_lifecycle.rs`: 241 lines
  - `agent/acp_session_requests.rs`: 123 lines

## Result

All listed checks passed.
