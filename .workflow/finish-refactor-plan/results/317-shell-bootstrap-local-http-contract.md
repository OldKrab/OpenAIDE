Packet ID: P317-shell-bootstrap-local-http-contract
Status: completed

Objective:
Add the shell/bootstrap contract that lets App Shells hand ephemeral LocalHttp
App Server connection info to the shared Frontend without moving endpoint
discovery or launch policy into browser code.

Changes:
- Added optional `WebviewAppServerConnection` bootstrap data for LocalHttp
  endpoint URL and process token.
- Added VS Code webview HTML serialization for the connection field.
- Added `connect-src` CSP generation for the LocalHttp endpoint origin when
  connection info is supplied.
- Updated shared Frontend bootstrap parsing to defensively parse the connection
  record and choose `createLocalHttpBackendConnection`.
- Reused the Frontend's existing sessionStorage-backed client instance id as
  the LocalHttp connection id.
- Kept the existing webview bridge as fallback when connection info is absent.

Verification:
- `npm run build --workspace @openaide/app-shell-contracts`
- `npm run check --workspace @openaide/app-shell-contracts`
- `npm run check --workspace openaide-frontend`
- `npm run test --workspace openaide-frontend -- src/services/hostBridge.test.ts`
- `npm run check --workspace openaide-vscode-extension`
- `npm run test --workspace openaide-vscode-extension -- src/webview/html.test.ts`
- `npm run check`
- `git diff --check`

Next:
Make the VS Code shell produce actual ephemeral LocalHttp connection info from
the shared attach-or-launch handoff while preserving the existing stdio child
path for elected launchers.
