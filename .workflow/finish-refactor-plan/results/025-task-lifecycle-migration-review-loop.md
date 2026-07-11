# P04 Task Lifecycle Migration Review Loop

Completed: 2026-06-26T22:37:42+03:00

## Doomsday Review Findings Fixed

- Fixed a resumed-session race: rejected or failed follow-up prompts no longer close an
  already-bound resumed native session.
- Fixed stale permission waiter routing by making the live waiter decision stable during
  the durable permission commit.
- Replaced a broad raw-`Store` creation validation callback with a narrow validation
  context.
- Moved lifecycle creation implementation into a child module so lifecycle fields stay
  private.
- Staged the new lifecycle module file so the committed diff is buildable.
- Replaced a false create rollback test with a real post-message-write failure test.
- Added permission routing failure/concurrency tests.
- Reduced `route_permission_response` visibility to `pub(crate)`.
- Moved create/adopt response snapshots into the Task mutation commit options and added
  a static guard against creation snapshot bypasses.

## Final Review Status

- Correctness subagent: no findings after fixes.
- Requirements/tests subagent: findings fixed and covered by tests.
- Code-quality subagent: findings fixed.

