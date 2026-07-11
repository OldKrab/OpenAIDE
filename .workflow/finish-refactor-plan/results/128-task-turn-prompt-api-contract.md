# Task Turn Prompt Split API Contract

## Decision

Split follow-up prompt orchestration out of `tasks/turn_lifecycle.rs` into
`tasks/turn_lifecycle/prompt.rs`.

This is a structural refactor only. Prompt behavior, storage mutation order,
Agent session handling, response snapshots, and turn spawning must not change.

## Module Boundary

`tasks/turn_lifecycle.rs` remains the lifecycle facade and owns:

- `TaskTurnLifecycle`;
- `TaskTurnLifecycle::new`;
- `TaskTurnLifecycle::create`;
- `TaskTurnLifecycle::cancel`;
- `TaskTurnLifecycle::respond_permission`;
- `TaskTurnLifecycle::shutdown`;
- `TaskTurnLifecycle::recover_volatile_runtime_state`;
- shared helper methods:
  - `start_session`;
  - `resume_session`;
  - `attach_session_events`;
  - `lock`;
  - `snapshot`;
  - `turn_is_still_active`;
  - `fail_created_task_start`;
  - `fail_adopted_task_attach`;
  - `transitions`;
- shared `snapshot_chat_commit_options`;
- shared `required_prompt_text`.

`tasks/turn_lifecycle/create.rs` remains responsible for:

- new prompt-start task creation;
- external session adoption;
- config option validation for task creation;
- create/adopt title helpers and create-specific commit options.

`tasks/turn_lifecycle/prompt.rs` owns:

- `TaskTurnLifecycle::prompt`;
- `AgentSessionPlan`;
- `AgentSessionPlan::from_task`;
- prompt-only imports and prompt-only local control flow.

## API Shape

`turn_lifecycle.rs` declares:

```rust
mod create;
mod prompt;
```

`prompt.rs` implements:

```rust
impl TaskTurnLifecycle {
    pub(crate) fn prompt(&self, params: SessionPromptParams) -> Result<TaskSnapshot, RuntimeError>;
}
```

No caller imports change. `TaskService` continues to call
`TaskTurnLifecycle::prompt` through the existing facade.

Prompt-only `AgentSessionPlan` moves into `prompt.rs`. Shared helpers stay in
the facade so `create.rs`, `prompt.rs`, and permission response logic can keep
using the same methods.

## Behavior That Must Stay Unchanged

- `SessionPromptParams` destructuring and field use stay unchanged.
- Prompt text validation still uses `required_prompt_text(params.text, "text")`.
- Prompt attachments are still cloned once for Agent delivery while the original
  attachment list is committed with the user message.
- A new `turn_id` is still allocated before session planning.
- The active-turn precheck still happens under `store_update_lock` before
  Agent session start/resume.
- `AgentSessionPlan::from_task` still starts a new session when the task has no
  `agent_session_id` and resumes when one exists.
- `AgentSessionStart` and `AgentSessionResume` request construction stays
  unchanged.
- `close_on_failure` remains true only for newly started sessions.
- Event attachment still happens before durable chat commit.
- Event attachment failure still closes only newly started sessions.
- Durable commit still:
  - rejects if another active turn appeared;
  - appends the user message;
  - appends the running turn activity message;
  - marks task status active;
  - sets `active_turn_id`;
  - stores the Agent session id;
  - updates timestamps;
  - uses `snapshot_chat_commit_options`.
- Rejected commit still closes newly started sessions and returns
  `InvalidParams("task already has an active turn")`.
- Commit errors still close newly started sessions and return the original
  error.
- Missing prompt snapshot still returns the same internal error text.
- Agent turn spawn still happens only after `turn_is_still_active` confirms the
  committed turn id is still active.
- The spawned Agent turn still receives the same task id, prompt text, Agent
  prompt attachments, turn id, and session.

## Test Expectations

Existing runtime and mutation tests cover this behavior. Run at least:

- `cargo test -p openaide-runtime task_create_and_follow_up_preserve_composer_context -- --nocapture`;
- `cargo test -p openaide-runtime prompt_rejects_double_turn_while_active -- --nocapture`;
- `cargo test -p openaide-runtime tasks::mutation::tests::task_turn_lifecycle_has_no_direct_commit_bypasses -- --nocapture`;
- `cargo test -p openaide-runtime`;
- `cargo fmt --all --check`;
- `npm run check`;
- `git diff --check`.

## Rejected Directions

- Do not change create/adopt flows in this slice.
- Do not move permission response logic in this slice.
- Do not introduce new prompt state structs beyond moving `AgentSessionPlan`.
- Do not hide close-on-failure ordering behind a generic helper unless review
  proves it is clearer.
- Do not change Agent session ownership, cancellation, or recovery behavior.

## Next Step

Implement the prompt split, then run doomsday-review against prompt ordering,
session cleanup behavior, commit behavior, and module isolation.
