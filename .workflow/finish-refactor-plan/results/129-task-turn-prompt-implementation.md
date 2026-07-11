# Task Turn Prompt Split Implementation

## Summary

Implemented the accepted Task Turn prompt split as a structural refactor with no
intended behavior changes.

## Code Changes

- Added `tasks/turn_lifecycle/prompt.rs` for:
  - `TaskTurnLifecycle::prompt`;
  - follow-up prompt session planning;
  - Agent session start/resume orchestration;
  - event attachment before durable chat commit;
  - prompt chat commit and snapshot mapping;
  - close-on-failure handling;
  - active-turn guard before spawning the Agent turn.
- Moved prompt-only `AgentSessionPlan` into `prompt.rs`.
- Kept `tasks/turn_lifecycle.rs` as the lifecycle facade for:
  - `TaskTurnLifecycle`;
  - constructor;
  - create routing;
  - cancel;
  - permission response;
  - shutdown and recovery;
  - shared helpers.
- Kept shared lifecycle helpers private after review confirmed child modules can
  call parent private items without widening visibility.

## Behavior Preservation

The implementation preserves:

- prompt text validation;
- prompt attachment clone and delivery behavior;
- active-turn precheck timing;
- Agent session start/resume request construction;
- close-on-failure behavior for newly started sessions only;
- event attachment before durable chat commit;
- durable user message and running activity construction;
- task active-turn field updates;
- response snapshot handling;
- active-turn guard before turn spawn.

## File Size Check

Production Rust files after the split:

- `tasks/turn_lifecycle.rs`: 197 lines;
- `tasks/turn_lifecycle/prompt.rs`: 164 lines.

Both are below the 400-line production source file limit.

## Next Step

Record the doomsday-review result and integration verification, then commit the
slice.
