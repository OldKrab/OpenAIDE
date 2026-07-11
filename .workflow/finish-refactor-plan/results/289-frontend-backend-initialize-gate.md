# A5f Frontend Backend Initialize Gate

Goal: enforce `client/initialize` at the Frontend BackendConnection bridge
before migrating typed product requests such as `task/open` and `task/list`.

## Scope

- Added `initialize(...)` to the VS Code webview BackendConnection bridge.
- `initialize(...)` sends typed `client/initialize` over the opaque
  `appServer.transport` bridge and stores the successful initialize result.
- `request(...)` now rejects product requests until initialize has succeeded.
- `close()` marks the bridge closed, rejects pending requests, clears
  initialization state, and prevents late initialize continuations from
  reopening the bridge.
- Concurrent initialize calls are coalesced to the same in-flight promise, and
  repeated initialize after success returns the stored result.

## Boundaries

- This slice does not wire App Controller initialization or migrate product
  intents. It only makes the transport contract safe for those follow-up
  slices.
- App Server events and snapshot ingestion are still not connected through this
  bridge in this slice.

## Verification

- `npm run test --workspace openaide-frontend -- backendConnectionBridge.test.ts` passed.
- `npm run check --workspace openaide-frontend` passed.
- `npm run test --workspace openaide-frontend` passed.
- `git diff --check` passed.

## Next

Continue A5 by adding the Frontend App Controller initialize lifecycle and
initial snapshot ingestion. Only after that should `task/open` and `task/list`
move to typed BackendConnection requests.
