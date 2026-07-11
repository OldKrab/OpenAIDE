# P322 Waitable Shell/Secret Producer Primitive

## Scope

Added typed waitable Backend producer primitives under `ServerRequestRuntime`
for client-scoped and task-scoped shell-owned requests.

## Decisions

- Waitable request state is separate from permission request state.
- `open_waitable_client_request` returns concrete deliveries; protocol-edge
  code is still responsible for sending them to the selected shell client.
- Task-scoped waitable requests may open before a shell responder is available;
  existing subscription/responder lifecycle delivers them when a client
  subscribes to that Task.
- Typed helpers exist for:
  - `secret/read`
  - `shell/showNotification`
- Timeout interrupts the pending request and removes wait state.
- ACP `secret_env` was not migrated in this slice because current Agent startup
  can block before protocol traffic delivers Backend-initiated requests. That
  needs an async preparation/session-start integration slice.

## Verification

- `cargo fmt --all --check`
- `cargo test -p openaide-runtime server_requests::runtime`

## Next

Integrate waitable shell/secret producer delivery through protocol-edge task
preparation/session startup so Agent secret lookup can move from legacy host
requests to typed task-scoped `secret/read` without deadlock.
