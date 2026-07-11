# P325 Task Update Server-Request Delivery Hook

## Scope

Added a connection-scoped protocol-edge hook that publishes a Task update and
drains server-request envelopes that became deliverable to that same connection.

## Decisions

- The delivery hook belongs in `RpcGateway`, not in Task product workflows.
- Stdio runtime `task.updated` handling now calls the single hook instead of
  manually composing Task events and server-request drain logic.
- The hook is intentionally connection-scoped because server-request envelopes
  are transport deliveries, while Task snapshots/events remain state-sync
  updates.
- Local HTTP still needs a separate push/poll transport decision for runtime
  notifications; this slice does not invent that transport behavior.

## Verification

- `cargo fmt --all --check`
- `cargo test -p openaide-runtime task_update_delivers_server_request_opened_after_subscription -- --nocapture`
- `cargo test -p openaide-runtime task_subscription_delivers_pending_server_request -- --nocapture`
- `cargo test -p openaide-runtime server_requests::runtime -- --nocapture`

## Next

Design and implement the App Server-owned opaque file-handle resolver needed
before `shell/revealFile` can open real files.
