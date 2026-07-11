# A5c Permission Server-Request Identity

Goal: make permission render state carry App Server server-request identity
explicitly, so Frontend can route typed permission responses without string
prefix guessing.

## Scope

- Added optional `app_server_request_id` to Backend normalized permission
  messages and `app_server_request_id?` to the shared presentation type.
- `TaskEventSink` fills `app_server_request_id` with the broker request id
  immediately after opening `permission/request`.
- Permission cards now respond with:
  - `app_server_request_id` and source `appServer` when present;
  - legacy Agent `request_id` and source `agent` otherwise.
- The central permission intent now uses the typed `BackendConnection.respond`
  path only for explicit App Server source, never by request-id prefix.

## Review Fixes

- Removed the unsafe `server-request-*` prefix discriminator from permission
  response routing.
- App Server-sourced responses without a `BackendConnection` now show a
  recoverable permission error instead of falling through to legacy
  `permission.respond` with the wrong id.
- Permission response UI state now uses `app_server_request_id` when present,
  matching the id dispatched by typed App Server responses.
- `ChatRow` callback typing now carries the explicit response source.
- Preserved legacy Agent permission response behavior for current or old
  snapshots that do not carry App Server request identity.

## Verification

- `cargo test -p openaide-runtime runtime_permission_request_reject_option_persists_denied_decision -- --nocapture` passed.
- `cargo check -p openaide-runtime` passed.
- `cargo fmt --check` passed.
- `npm run build --workspace @openaide/app-shell-contracts` passed.
- `npm run check --workspace @openaide/app-shell-contracts` passed.
- `npm run test --workspace openaide-frontend -- ChatMessageView.test.tsx TaskView.test.ts appControllerCallbacks.test.ts` passed.
- `npm run test --workspace openaide-frontend` passed.
- `npm run check --workspace openaide-frontend` passed.

## Next

Continue A5 by removing the remaining legacy permission response fallback once
all active render surfaces are sourced from snapshots that include
`app_server_request_id`, then migrate Task lifecycle intents to
`BackendConnection.request(...)`.
