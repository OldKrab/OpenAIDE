# P08 Task Service Facade Implementation

Completed: 2026-06-27T02:28:51+03:00

## Implemented

- Split read-only Task facade methods into `TaskQueries`.
- Added `TaskReadStore` so query code receives only the read operations it needs
  instead of full `Store` write authority.
- Split non-turn Task commands into `TaskCommands`.
- Kept `TaskService` as the public facade with unchanged method signatures and
  protocol-facing behavior.
- Left create, prompt, cancel, permission response, shutdown turn cleanup, and
  volatile recovery in `TaskTurnLifecycle`.
- Left Agent probe/auth/list/config operations in `TaskService`.
- Kept mark-read and archive/restore/delete durable writes behind
  `TaskMutations::commit_existing_task`.

## Tests Added Or Updated

- Added boundary tests that keep `TaskQueries` away from mutation seams and direct
  storage write authority.
- Added boundary tests that keep `TaskReadStore` read-only and keep `TaskCommands`
  routed through `TaskMutations`.
- Updated the production source-size scanner so separate Rust test files are not
  counted as production source.
