# P320 Server Request Category Typing

## Scope

Typed the remaining top-level Backend-initiated request categories in the App
Server Protocol source and generated TypeScript bindings:

- `secret/read`
- `shell/showNotification`
- `shell/revealFile`

## Decisions

- `BackendConnection.respond(requestId, result)` stays the generic response
  seam.
- Generated method maps now type each Backend-initiated request method's params
  and response result.
- `shell/revealFile` carries an opaque `fileHandleId` plus optional safe label,
  never a raw local path.

## Verification

- `cargo fmt --all --check`
- `cargo test -p openaide-app-server-protocol server_requests`
- `cargo test -p openaide-app-server-protocol typescript`
- `npm run protocol:generate`
- `npm run protocol:check`
- `npm run test --workspace @openaide/app-server-client`
- `git diff --check`

## Next

Implement runtime routing and shell capability handling for typed
`secret/read`, `shell/showNotification`, and `shell/revealFile` requests.
