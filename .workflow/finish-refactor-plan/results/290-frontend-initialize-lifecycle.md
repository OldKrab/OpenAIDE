# Frontend Initialize Lifecycle

Status: implemented, reviewed, verified, and committed-ready.

## Scope

- Added Frontend client instance identity generation using browser `sessionStorage` with memory fallback.
- Added typed `client/initialize` parameter construction from the current webview surface.
- Wired App Controller startup to call `BackendConnection.initialize` before migrated product requests are allowed.
- Added transitional ingestion of App Server initial task navigation and active Task snapshots into current render state.
- Kept legacy host-message startup as the visible fallback during migration.
- Closed the Backend connection on controller unmount.

## Review Fixes

- Prevented late initial App Server snapshots from overwriting accepted fresher legacy startup state by suppressing only slices whose accepted legacy reducer actions already arrived.
- Ensured rejected legacy snapshots or archive-mismatched list responses do not suppress valid initialize state.
- Added controller cleanup through `BackendConnection.close()`.

## Verification

- `npm run test --workspace openaide-frontend -- appController.test.tsx appServerInitialSnapshot.test.ts backendInitialization.test.ts`
- `npm run check --workspace openaide-frontend`
- `npm run test --workspace openaide-frontend`
- `git diff --check`
- Subagent review after fixes: no findings.
