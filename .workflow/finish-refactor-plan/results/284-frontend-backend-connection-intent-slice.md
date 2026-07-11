# A5a Frontend BackendConnection Intent Slice

Goal: establish the typed shared Frontend/App Server connection contract and
move one live mutating UI path through the central intent layer without waiting
for the full Frontend migration.

## Scope

- Added generated `permission/request` Backend-initiated request and response
  protocol types from the Rust protocol source.
- Added `BackendConnection` to `@openaide/app-server-client` with the accepted
  seam shape:
  - `initialize(params, meta)`;
  - `request(method, params, meta)`;
  - `events(listener)`;
  - `respond(requestId, result)`;
  - `close()`.
- Added `backendRequest(...)` as a typed request record helper over generated
  protocol method names, params, and results.
- Added `packages/frontend/src/intents/taskIntents.ts` as the first central
  Task intent module.
- Moved permission response callbacks through
  `respondToPermissionIntent(...)`.
- When a `BackendConnection` is injected, permission responses now use
  `backendConnection.respond<typeof PERMISSION_REQUEST>(requestId, { optionId })`.
- The existing host bridge `permission.respond` path remains only as the
  current shell fallback until the shell is connected to the App Server
  Protocol.

## Boundaries

- Rendering callbacks no longer own permission response transport details.
- The new intent owns pending presentation and recoverable response failure
  dispatch for the typed path.
- `useAppController` accepts an optional injected `BackendConnection` so the
  next shell integration slice can wire a concrete transport without changing
  rendering callbacks again.
- This slice does not migrate `task/create`, `task/send`, `task/cancel`,
  navigation, settings, snapshots, subscriptions, or shell bootstrap.
- `app-shell-contracts` still contains legacy product bridge messages for
  unmigrated paths; shrinking it is part of the remaining A5 work.

## Review Fixes

- Synchronous `BackendConnection.respond` throws now dispatch a recoverable
  permission error instead of leaving the request stuck in responding state.
- Permission response payloads now use generated protocol response typing
  instead of an arbitrary JSON value.
- The shared controller now has an explicit `BackendConnection` injection point;
  the concrete shell transport remains the next A5 substep.
- `PermissionRequestParams` no longer declares its own request id; the
  Backend-initiated request envelope remains the only request identity.

## Verification

- `npm run protocol:generate` passed.
- `npm run protocol:check` passed.
- `cargo test -p openaide-app-server-protocol` passed.
- `npm run test --workspace @openaide/app-server-client` passed.
- `npm run test --workspace openaide-frontend -- appControllerCallbacks.test.ts`
  passed.
- `npm run build --workspace @openaide/app-server-client` passed.
- `npm run check --workspace @openaide/app-server-client` passed.
- `npm run check --workspace openaide-frontend` passed.
- `npm run test --workspace openaide-frontend` passed.
- `cargo fmt --check` passed.
- `git diff --check` passed.

## Next

Continue A5 by wiring a real App Shell `BackendConnection` implementation into
the shared Frontend bootstrap, then migrate Task lifecycle intents one group at
a time from legacy host messages to typed App Server Protocol requests.
