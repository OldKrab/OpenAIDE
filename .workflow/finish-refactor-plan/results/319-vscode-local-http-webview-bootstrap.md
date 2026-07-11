Packet ID: P319-vscode-local-http-webview-bootstrap
Status: completed

Objective:
Wire VS Code webview surfaces to request the runtime LocalHttp handoff and pass
the returned ephemeral connection info into Frontend bootstrap without freezing
or starting the app with partial transport state.

Changes:
- Added immediate preparing HTML for webviews while handoff runs.
- Task editor and navigation surfaces now call
  `RuntimeProcess.startAppServerConnection()` before rendering the normal app.
- Successful handoff injects LocalHttp connection info into bootstrap.
- Failed handoff logs a warning and renders the existing bridge-backed app
  bootstrap.
- New Task adoption preserves the current LocalHttp connection when re-rendering
  the adopted Task panel.
- Pending async handoff renders are generation-guarded so disposed or
  superseded webviews are not mutated after completion.
- Final bootstrap is rebuilt after handoff completes so preferences and Agent
  catalog data are fresh.

Review:
- Bounded subagent review found stale/disposed render risk and stale bootstrap
  capture; both were fixed before commit with regression tests.

Verification:
- `npm run test --workspace openaide-vscode-extension -- src/webview/surfaces.test.ts src/webview/html.test.ts`
- `npm run check --workspace openaide-vscode-extension`

Next:
Review whether A7 still needs a small transport lifecycle cleanup before moving
back to A4 shell/secret server-request categories.
