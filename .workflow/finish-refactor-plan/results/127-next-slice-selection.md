# Next Slice Selection: Task Turn Prompt Split

## Selected Slice

Split follow-up prompt orchestration out of `tasks/turn_lifecycle.rs` into a
focused child module.

Tentative module shape:

- `tasks/turn_lifecycle.rs`: lifecycle facade, constructor, `create`, `cancel`,
  `respond_permission`, `shutdown`, recovery entry point, shared lifecycle
  helpers, and shared prompt text validation.
- `tasks/turn_lifecycle/create.rs`: new task creation and external session
  adoption flows, unchanged.
- `tasks/turn_lifecycle/prompt.rs`: follow-up prompt flow for existing tasks,
  including:
  - `TaskTurnLifecycle::prompt`;
  - follow-up session plan derivation;
  - prompt-specific commit result mapping;
  - prompt-specific session close-on-failure handling;
  - prompt turn spawn after commit.

## Why This Slice

`tasks/turn_lifecycle.rs` still mixes several responsibilities:

- lifecycle facade construction;
- task creation routing;
- follow-up prompt orchestration;
- cancel and permission response routing;
- shared Agent session helpers;
- snapshot and active-turn checks;
- follow-up session-plan derivation.

The create/adopt flows already live in `turn_lifecycle/create.rs`. Splitting the
follow-up prompt flow into a sibling module gives Task turn lifecycle the same
shape: each major user action owns its local orchestration while the facade file
keeps shared helpers and public routing.

This matters because prompt send is a high-risk user path: it crosses task
storage, Agent session start/resume, event attachment, chat commit, snapshot
response, and live turn spawn. Keeping it isolated makes later recovery,
attachments, and responsive pending-state work easier to review.

## Intended Boundary

`tasks/turn_lifecycle.rs` should keep:

- `TaskTurnLifecycle`;
- `TaskTurnLifecycle::new`;
- `TaskTurnLifecycle::create`;
- `TaskTurnLifecycle::cancel`;
- `TaskTurnLifecycle::respond_permission`;
- `TaskTurnLifecycle::shutdown`;
- `TaskTurnLifecycle::recover_volatile_runtime_state`;
- shared helper methods used by create and prompt flows:
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

`tasks/turn_lifecycle/prompt.rs` should own:

- `TaskTurnLifecycle::prompt`;
- `AgentSessionPlan`;
- `AgentSessionPlan::from_task`;
- any prompt-only helper introduced while preserving behavior.

## Constraints

- No behavior changes.
- Keep `SessionPromptParams` handling unchanged.
- Keep prompt text validation unchanged.
- Keep active-turn precheck and rejected commit mapping unchanged.
- Keep Agent session start/resume request construction unchanged.
- Keep close-on-failure behavior unchanged.
- Keep event attachment timing unchanged.
- Keep user message and running activity message construction unchanged.
- Keep task field updates unchanged.
- Keep response snapshot behavior unchanged.
- Keep `turn_is_still_active` guard before spawning Agent turn unchanged.
- Keep prompt attachment cloning and Agent delivery behavior unchanged.
- Keep production Rust source files under the 400-line limit.

## Main Risks To Grill

- Whether `AgentSessionPlan` should move with prompt or remain shared in the
  facade. It is currently only used by follow-up prompts, so moving it should be
  cleaner.
- Whether `snapshot_chat_commit_options` should stay shared because permission
  response also uses it.
- Whether prompt-only session close-on-failure logic deserves a private helper
  in `prompt.rs` or should remain inline to avoid hiding ordering.
- Whether tests currently cover enough prompt behavior after the move or need a
  focused boundary check.

## Next Step

Grill and record the API contract for the Task Turn prompt split.
