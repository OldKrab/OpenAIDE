# P423 Fast Remaining Gap Audit

## Status

Completed.

## Finding

The first apparent unfinished accepted slice was stale documentation, not a code
gap. The ACP session close/delete termination split was already implemented in
commit `887a729`.

Current code confirms:

- `agent/acp_session_termination.rs` owns close/delete helpers.
- `agent/acp_session_termination/tests.rs` owns termination boundary tests.
- `agent/acp_session_lifecycle.rs` keeps startup, load/replay, list-session,
  and projection helpers.

## Change

Updated `docs/refactor-plan.md` with the missing implementation status for the
ACP session termination split.

## Next Packet

P424 audits the remaining top-level Storage And Lifecycle section and selects
the smallest concrete gap around concurrent storage protection, shared App
Server lifecycle, or recovery.

## Verification

No product code changed.

Checked:

- `git log --oneline -- agent/acp_session_termination.rs`
- `agent/acp_session_lifecycle.rs`
- `agent/acp_session_termination.rs`
- `rg "close_active_session|delete_active_session" openaide-rs/app-server/src/agent`
