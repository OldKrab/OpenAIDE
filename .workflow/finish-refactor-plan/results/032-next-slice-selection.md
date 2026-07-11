# P11 Next Slice Selection

Completed: 2026-06-27T02:33:52+03:00

## Selected Slice

Split Agent-facing public operations out of `TaskService` into an internal
`AgentService`.

## Why This Slice

- The previous slice moved Task reads and non-turn Task commands out of
  `TaskService`; the remaining non-task-workflow block in `TaskService` is Agent
  probe/auth/session-list/config option handling.
- Moving those methods is a narrow backend refactor with clear protocol-contract
  coverage.
- It reduces `TaskService` toward a facade/composition root without changing App Server
  Protocol method names or transport behavior.

## Scope

- Move public Agent operations currently implemented on `TaskService` into
  `AgentService`.
- Keep `TaskService` methods as stable facade delegates for the current transport
  dispatcher.
- Keep Agent session start/load/resume/close, Native Session lifecycle policy, turn
  spawning, and Task creation in Task workflow modules.
- Do not change protocol names, params, result types, or Frontend behavior.

## Main Risk

Do not accidentally split live Agent session lifecycle policy away from Task ownership.
This slice is only about public Agent utility operations and prepared option requests,
not active Task execution.
