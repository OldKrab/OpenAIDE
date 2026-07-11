# Frontend Safe Task Mutations

Status: implemented, reviewed, verified, and committed-ready.

## Scope

- Added a central Frontend task mutation intent module.
- Routed safe text-only active Task sends through typed `task/send`.
- Routed Task cancel through typed `task/cancel`.
- Kept attachment sends on legacy transport until App Server attachment handles exist.
- Kept new Task create, new Task config, archive/restore, and other unsupported mutation paths on legacy transport because current UI state does not yet match the App Server Protocol inputs.

## Review Fixes

- Removed unsafe legacy replay after a typed mutation request has been attempted.
- Left transport-style unknown `task/send` failures in submitted/pending state to avoid duplicate manual resends with a fresh idempotency key.
- Restored the composer only for authoritative `AppServerProtocolError` rejections.

## Verification

- `npm run test --workspace openaide-frontend -- appControllerCallbacks.test.ts`
- `npm run check --workspace openaide-frontend`
- `npm run test --workspace openaide-frontend`
- `git diff --check`
- Subagent review after fixes: no findings.
