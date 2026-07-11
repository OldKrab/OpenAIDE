# ACP Session Worker Wiring Review Loop

Ran `$doomsday-review` with independent correctness, requirements/tests, and code-quality passes.

## First Review

- Correctness: no findings.
- Code quality: no findings.
- Requirements/tests: one medium finding. The extracted connection wiring was only partially covered by tests: replay capture and one `read_text_file` host route were covered, but the other moved handler registrations were not.

## Fix

- Moved connection tests to `agent/acp_session_connection/tests.rs`.
- Expanded coverage to exercise:
  - matching load replay capture;
  - nonmatching session notifications not entering replay capture;
  - `session/request_permission` registration;
  - `fs/read_text_file`;
  - `fs/write_text_file`;
  - `terminal/create`;
  - `terminal/output`;
  - `terminal/wait_for_exit`;
  - `terminal/kill`;
  - `terminal/release`.

## Rerun

Reran the requirements/tests doomsday pass after the fix. It found one narrower
test weakness: the replay test asserted count and variant but not that the
captured chunk was the matching-session text.

## Second Fix

- Tightened the replay assertion to verify the captured text is `replayed`, so
  capturing the nonmatching `ignored` notification would fail the test.

## Final Rerun

Reran the requirements/tests doomsday pass after the second fix. It found one
low coverage gap: trace recording and the unhandled-notification fallback
`retry: false` decision were still unprotected.

## Third Fix

- Extracted private helper decisions for `session/update` handling and unhandled
  fallback construction.
- Added a focused test that enables ACP tracing, verifies a `session/update`
  trace line, verifies an unmatched update is forwarded, and verifies the
  fallback is `Handled::No` with `retry: false`.

## Final Rerun

Reran the requirements/tests doomsday pass after the third fix. Result: no
findings.
