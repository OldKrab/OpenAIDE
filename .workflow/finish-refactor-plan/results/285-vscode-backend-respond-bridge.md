# A5b VS Code Backend Respond Bridge

Goal: wire the first real App Shell bridge for `BackendConnection.respond`
without migrating the rest of Frontend to App Server Protocol yet.

## Scope

- Added a generic opaque `appServer.transport` webview-to-host bridge message.
- Added `createWebviewBackendResponder(...)` in shared Frontend services.
- `hostBridge` now injects that responder only when running inside VS Code;
  standalone/dev host remains on legacy paths.
- `useAppController` now uses an injected responder option when provided, or
  the VS Code bridge responder when available.
- `RuntimeClient.respondServerRequest(...)` writes a JSON-RPC response with the
  server-request id directly to the runtime process.
- VS Code webview messaging routes `appServer.respond` to the runtime client.
- Safe action logging now includes `request_id` for `appServer.transport`.

## Boundaries

- `app-shell-contracts` defines only the opaque shell bridge message; generated
  App Server Protocol request/response typing stays in `@openaide/app-server-client`.
- The shared Frontend adapter types `respond` with generated
  `ServerRequestResponseResultByMethod`.
- This slice still does not implement `initialize`, `request`, event
  subscription, state ingestion, or Task lifecycle protocol intents.
- Current chat permission cards still use legacy Agent request ids. The typed
  response path requires an explicit App Server source until snapshots expose
  renderable server-request identity for those cards.

## Review Fixes

- Removed prefix-based request-id routing; current permission cards stay on the
  legacy path unless App Server source is explicit.
- Replaced typed `appServer.respond` shell-contract payload semantics with an
  opaque `appServer.transport` envelope parsed by the VS Code shell.
- Runtime errors for `appServer.transport` now preserve the request id so
  Frontend can clear responding state.

## Verification

- `npm run build --workspace @openaide/app-shell-contracts` passed.
- `npm run check --workspace @openaide/app-shell-contracts` passed.
- `npm run test --workspace openaide-frontend -- backendConnectionBridge.test.ts appControllerCallbacks.test.ts` passed.
- `npm run test --workspace openaide-frontend` passed.
- `npm run check --workspace openaide-frontend` passed.
- `npm run test --workspace openaide-vscode-extension -- rpcClient.test.ts messaging.test.ts` passed.
- `npm run test --workspace openaide-vscode-extension` passed.
- `npm run check --workspace openaide-vscode-extension` passed.
- `git diff --check` passed.

## Next

Continue A5 by exposing server-request identity in renderable Task state for
permission cards, then remove the legacy permission fallback. After that,
migrate Task lifecycle intents to typed `BackendConnection.request(...)` in
small groups.
