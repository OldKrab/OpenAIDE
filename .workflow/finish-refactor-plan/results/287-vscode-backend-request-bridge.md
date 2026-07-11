# A5d VS Code Backend Request Bridge

Goal: add the generic request half of the local VS Code App Server bridge so
shared Frontend can call typed `BackendConnection.request(...)` before product
intents are migrated.

## Scope

- Extended the opaque `appServer.transport` bridge to carry host-to-webview
  responses as well as webview-to-host requests and responses.
- `createWebviewBackendConnection(...)` now exposes typed `request(...)` and
  `respond(...)` over generated `@openaide/app-server-client` method maps.
- Frontend request promises resolve or reject from matching opaque transport
  responses.
- The bridge subscription is lazy and repeatable: it subscribes when requests
  are pending, unsubscribes when idle, and resubscribes for later requests.
- The bridge implements `close()` and request timeouts so abandoned requests
  reject and idle subscriptions are cleaned up.
- `RuntimeClient.appServerRequest(...)` forwards generated App Server Protocol
  method names, params, and optional meta through the runtime JSON-RPC stream.
- The VS Code runtime boundary unwraps App Server `ResponseEnvelope.result`
  before returning typed product results to the Frontend bridge.
- App Server protocol errors are carried as structured `ErrorEnvelope` values
  through runtime, webview messaging, and Frontend rejection as
  `AppServerProtocolError`.
- VS Code webview messaging routes opaque request payloads to the runtime and
  posts opaque transport response payloads back to the webview.
- Runtime stream writing and line-reader attachment are isolated from
  `RuntimeClient` to keep the runtime client below the production source-file
  split threshold.

## Boundaries

- `app-shell-contracts` still only defines an opaque `appServer.transport`
  payload; App Server method/result typing stays in `@openaide/app-server-client`.
- No product Task UI state is migrated in this slice. `task/list`, `task/open`,
  `task/create`, `task/send`, and related intents still use existing legacy
  host messages until state mapping/ingestion is wired.
- Existing `respond(...)` behavior for Backend-initiated requests is preserved.

## Verification

- `npm run build --workspace @openaide/app-server-client` passed.
- `npm run test --workspace @openaide/app-server-client` passed.
- `npm run check --workspace @openaide/app-server-client` passed.
- `npm run build --workspace @openaide/app-shell-contracts` passed.
- `npm run check --workspace @openaide/app-shell-contracts` passed.
- `npm run test --workspace openaide-frontend -- backendConnectionBridge.test.ts` passed.
- `npm run check --workspace openaide-frontend` passed.
- `npm run test --workspace openaide-frontend` passed.
- `npm run test --workspace openaide-vscode-extension -- rpcClient.test.ts messaging.test.ts` passed.
- `npm run test --workspace openaide-vscode-extension` passed.
- `npm run check --workspace openaide-vscode-extension` passed.
- `git diff --check` passed.

## Next

Continue A5 by mapping App Server Protocol task snapshots/navigation into the
current Frontend render state, then migrate `task/open` and `task/list` intents
to the typed request bridge before moving to mutating `task/*` intents.
