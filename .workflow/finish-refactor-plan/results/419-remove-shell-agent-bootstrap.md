# P419 - Remove Shell Agent Bootstrap

## Result

Removed the stale shell-provided Agent list from Webview bootstrap. Frontend now gets Agent product state from App Server initialization instead of VS Code configuration/bootstrap data.

## Changes

- Removed `WebviewBootstrap.agents` and `WebviewAgentOption`.
- Removed VS Code `data-agents` HTML serialization and Frontend parsing.
- Deleted the VS Code webview Agent catalog collector.
- Removed standalone dev-host demo Agents from bootstrap data.
- Kept Agent secret key helpers untouched because Backend-initiated secret requests still use them.

## Verification

- `npm run build --workspace @openaide/app-shell-contracts`
- `npm run check --workspace openaide-frontend`
- `npm run check --workspace openaide-vscode-extension`
- `npm run test --workspace openaide-frontend -- appController hostBridge devHost AppSurfaces`
- `npm run test --workspace openaide-vscode-extension`
- `git diff --check`
- Production source-size guard, excluding tests

## Next

P420 should fast-pick the next stale product-state owner or fallback path and close it in the smallest verified commit.
