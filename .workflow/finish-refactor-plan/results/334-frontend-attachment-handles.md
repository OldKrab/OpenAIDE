# P334 Frontend Attachment Handles

## Result

- Frontend task prompt sends now use typed `task/send` when all composer attachments have App Server handle ids.
- Raw/path-backed legacy composer attachments still fall back to the host bridge.
- Legacy prompt serialization strips App Server-only handle metadata.
- Removing a task composer attachment now removes the row immediately and releases App Server-owned pre-send handles asynchronously.

## Verification

- `npm run check --workspace openaide-frontend`
- `npm run test --workspace openaide-frontend -- appControllerCallbacks.test.ts AppSurfaces.test.tsx`
- `npm run check --workspace @openaide/app-server-client`
- `git diff --check`

## Remaining

- Embedded snapshot candidates and confirmation.
- File browser composer UI that creates App Server handles instead of relying on legacy raw attachments.
