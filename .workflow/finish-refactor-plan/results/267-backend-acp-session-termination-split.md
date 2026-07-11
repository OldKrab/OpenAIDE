# P245 Backend ACP Session Termination Split

## Contract

Split ACP session close/delete termination helpers out of
`openaide-rs/app-server/src/agent/acp_session_lifecycle.rs` while preserving
session lifecycle startup, load, list, and projection helpers in the lifecycle
module.

Move `close_active_session` and `delete_active_session` into a focused private
termination module. Preserve trace direction and event names, support-capability
checks, close no-op when unsupported, delete `CapabilityMissing` text when
unsupported, request construction, response trace recording, error trace
recording, ACP error mapping, `AcpSessionRunner` public behavior, and existing
tests.

Do not change active-session startup, load/resume replay, list-sessions
filtering, options sessions, prompt running, Agent runtime behavior, storage,
protocol shapes, or App Server lifecycle in this slice.

## Status

Completed.

## Implementation

- Added `openaide-rs/app-server/src/agent/acp_session_termination.rs`.
- Moved `close_active_session` and `delete_active_session` out of
  `acp_session_lifecycle.rs`.
- Updated ACP prompt/session runners to import termination helpers from the new
  focused module.

## Review

Round 1 found one accepted Medium requirements/test gap: existing tests cover
supported successful close/delete dispatch, but do not prove unsupported close
no-op behavior, unsupported delete error text, close/delete trace event names,
ACP error mapping on failed delete, or closing while the prompt runner owns the
close path.

Fix: added focused `acp_session_termination` tests for unsupported close,
unsupported delete, exact trace event/direction pairs, and failed delete ACP
error mapping. Added an active-session runtime test for closing while a prompt
is still running.

Round 2 found the trace assertion was too loose because it did not bind event
names to directions. Fixed by parsing JSONL trace records and asserting exact
`(event, direction)` pairs.

## Verification

- `cargo fmt --all --check`
- `cargo check -p openaide-runtime`
- `cargo test -p openaide-runtime agent::acp_session_termination -- --nocapture`
- `cargo test -p openaide-runtime close_session_dispatches_while_prompt_is_running -- --nocapture`
- `cargo test -p openaide-runtime agent::acp::tests -- --nocapture`
- `npm run check`
- `npm test`
- `git diff --check`
- `jq empty .workflow/finish-refactor-plan/state.json`
- Production source-size scan, excluding tests/generated/dist/examples/target/node_modules.

## Next

Start A1: grill and implement the live runtime entrypoint migration to the App
Server Protocol edge.
