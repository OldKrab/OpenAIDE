# Frontend Typed Task Read Path

Status: implemented, reviewed, verified, and committed-ready.

## Scope

- Added a central Frontend task read intent module for typed `task/list` and `task/open` requests through `BackendConnection.request`.
- Used typed task reads only after `client/initialize` succeeds.
- Requested typed task navigation only when initialize omitted navigation state.
- Requested typed task open only when initialize omitted active Task state or a post-initialize fallback is needed.
- Preserved legacy `task.markRead` startup behavior on Task surfaces so unread semantics do not regress.
- Kept legacy host-message fallback for unavailable or failed typed read paths.

## Review Fixes

- Prevented typed `task/open` fallback before initialize succeeds.
- Preserved Task-surface legacy `task.markRead` semantics.
- Guarded typed task-list results against archive generation changes.
- Guarded initialize-provided task navigation against archive generation changes.
- Prevented initialize failure fallback from posting after controller unmount.

## Verification

- `npm run test --workspace openaide-frontend`
- `npm run check --workspace openaide-frontend`
- `git diff --check`
- Production file size check: `appController.ts` is 299 lines.
- Subagent review after fixes: no findings.
