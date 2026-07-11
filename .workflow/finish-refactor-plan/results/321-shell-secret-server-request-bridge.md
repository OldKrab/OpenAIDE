# P321 Shell/Secret Server Request Bridge

## Scope

Added the Frontend/App Shell bridge for typed Backend-initiated shell-owned
requests:

- `secret/read`
- `shell/showNotification`
- `shell/revealFile`

## Decisions

- Permission requests remain user-facing Task UI requests and are not handled by
  the shell capability bridge.
- App Shell request/result messages are explicit shell contracts, not normal
  product protocol requests.
- VS Code can answer `secret/read` and `shell/showNotification` today.
- `shell/revealFile` returns `{ revealed: false }` until Backend owns opaque
  file-handle resolution; no raw path fallback was added.

## Verification

- `npm run check`
- `npm run check --workspace openaide-frontend`
- `npm run check --workspace openaide-vscode-extension`
- `npm test --workspace openaide-frontend -- appServerServerRequests`
- `npm test --workspace openaide-vscode-extension -- messaging`
- `git diff --check`

## Next

Add Backend-side request producers for the shell/secret categories that are now
typed and bridged, starting with Agent runtime secret lookup or notification
needs. Design App Server-owned opaque file-handle resolution before enabling
real `shell/revealFile` opening.
