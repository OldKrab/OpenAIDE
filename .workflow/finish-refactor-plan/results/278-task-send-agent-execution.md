# A3g Task Send Agent Execution

## Contract

Make App Server Protocol `task/send` start real Agent execution after durable
acceptance instead of leaving a persistence-only running turn.

- Open or resume the Task's Native Session before committing the turn.
- Attach session event sinks before accepting the turn.
- Commit the user message, running turn, and bound session id atomically through
  the existing mutation boundary.
- Spawn Agent prompt execution only after durable acceptance.
- If session start or event attachment fails, do not commit Chat or active-turn
  state.
- Keep `task/create` prompt-free.

## Status

Completed.

## Implementation

- Added Agent runtime dependencies to `TaskProductApi`.
- Split Agent session opening into `tasks::product_api::send::session`.
- Wired `task/send` to start/resume Agent sessions, persist the session binding,
  and spawn `TurnRunner` after commit.
- Split protocol-edge stdio gateway construction into `stdio::factory`.
- Added test-only stdio Agent injection so protocol-edge unit tests do not start
  real ACP processes.
- Added product API tests for Agent prompt execution and start-failure atomicity.
- Added RuntimeNotifier-driven protocol-edge stdio task update events for
  background Agent completion.
- Added live turn cancellation through `TurnRunner` for `task/cancel`.
- Added production host bridge forwarding for protocol-edge stdio ACP requests.
- Added RAII cleanup for newly started sessions before durable acceptance.
- Split stdio wire handling into `stdio::wire`.

## Review

- Initial review found stale UI events after fast Agent completion, live Agent
  cancellation missing from `task/cancel`, disabled stdio host capabilities,
  manual new-session cleanup, and missing side-effect tests.
- Fixes added RuntimeNotifier-driven `app/event` publication, live
  `TurnRunner` cancellation, host request forwarding and host response
  handling, RAII session cleanup, and direct regression tests.
- Follow-up review found stale revision revalidation missing after Agent
  session open and a weak background-completion event test.
- Final fixes revalidated Task revision inside the mutation, avoided
  overwriting committed config options with session values, and strengthened the
  completion-event test to wait for a non-running Task event.
- Final correctness, requirements/tests, and code-quality reviews reported no
  findings.

## Verification

- `cargo fmt --all`
- `cargo check -p openaide-runtime`
- `cargo test -p openaide-runtime tasks::product_api -- --nocapture`
- `cargo test -p openaide-runtime protocol_edge -- --nocapture`
- `cargo fmt --all --check`
- `cargo test --workspace -- --test-threads=1`
- `npm run check`
- `npm run test --workspace @openaide/app-server-client`
- `git diff --check`
- `jq empty .workflow/finish-refactor-plan/state.json`

## Next

Commit this sub-slice, then continue A3 with responsive
preparation/readiness snapshots.
