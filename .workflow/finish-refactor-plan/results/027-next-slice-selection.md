# P06 Next Slice Selection

Completed: 2026-06-26T22:40:16+03:00

## Selected Slice

Split the current `TaskService` facade into explicit Backend service boundaries without
changing protocol behavior.

## Why This Slice

`TaskService` currently owns too many unrelated responsibilities:

- construction and recovery wiring;
- read-only Task queries and Chat paging;
- Task commands such as mark-read and delete;
- turn lifecycle delegation;
- Agent status/config/session operations.

That makes later App Server module work harder because protocol dispatch only sees one
large service object and tests naturally reach through that object for every concern.
The next slice should make the public Backend boundary more explicit while preserving
existing method behavior.

## Chosen Direction

Keep `TaskService` as the compatibility facade used by `Runtime` and protocol
dispatch, but make it a thin owner/composer over smaller internal modules:

- `TaskQueries` for read-only Task, Chat, diagnostics, and tool-detail queries.
- `TaskCommands` for non-turn Task mutations such as mark-read and archive/restore/delete.
- Existing `TaskTurnLifecycle` for create, prompt, cancel, permission response,
  shutdown turn cleanup, and recovery.
- A later `AgentService` or `AgentWorkflows` slice for Agent probe/auth/list/config
  operations.

The first implementation should extract only Task read/query and non-turn command
boundaries. Agent service extraction is intentionally deferred so this slice stays
small and reviewable.

## Non-Goals

- Do not change protocol method names, params, results, or transport dispatch.
- Do not redesign Agent runtime or ACP behavior.
- Do not move Task turn lifecycle again except to depend on the same shared context.
- Do not introduce generic service containers or dependency-injection frameworks.

## Next

Proceed to API grilling for the `TaskService` facade split before implementation.

