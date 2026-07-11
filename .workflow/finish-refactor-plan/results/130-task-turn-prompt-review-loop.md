# Task Turn Prompt Split Review Loop

## Review Method

Ran `$doomsday-review` against the Task Turn prompt split using correctness,
requirements/tests, and code-quality subagent passes, plus a local prompt
ordering pass because this path crosses Agent session ownership, storage commit,
and turn spawning.

## Findings Fixed

1. Low requirement gap: `prompt.rs` initially owned only a private
   `prompt_existing_task` helper while the public `TaskTurnLifecycle::prompt`
   method remained in `turn_lifecycle.rs`.
   - Fix: moved `pub(crate) fn prompt` into `prompt.rs` and removed the delegate.

2. Low module isolation issue: helper methods in `turn_lifecycle.rs` were
   temporarily widened to `pub(super)`.
   - Fix: reverted the helper methods and shared functions back to private.

3. Low required-check failure: formatting drift after the review fixes.
   - Fix: ran `cargo fmt --all` and reran the required formatting check.

## Final Review Result

After fixes:

```text
Findings

No findings.

Summary: 0 findings: 0 correctness, 0 requirements/tests, 0 code quality.
```

## Local Ordering Pass

The targeted local pass checked that prompt order stayed unchanged:

1. validate prompt text;
2. clone prompt attachments for Agent delivery;
3. allocate turn id;
4. acquire lock and precheck active turn;
5. derive start/resume session plan;
6. start or resume Agent session;
7. attach session events before durable chat commit;
8. commit user message, running activity, active task state, turn id, session
   id, and timestamps;
9. map rejected commit and errors with the same close-on-failure behavior;
10. build response snapshot;
11. check `turn_is_still_active`;
12. spawn Agent turn with the same prompt data.

## Next Step

Run final verification and commit the implementation.
