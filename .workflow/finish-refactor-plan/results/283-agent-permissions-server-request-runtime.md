# A4b Agent Permissions Server-Request Runtime

Goal: route live Agent permission prompts through the shared App Server
server-request runtime instead of the old `TurnRunner`-local waiter registry.

## Scope

- Added `ServerRequestRuntime` as the shared owner of:
  - `ServerRequestBroker` state;
  - live permission waiters;
  - Agent request id to App Server request id mapping;
  - option validation, first-response-wins, and cancellation cleanup.
- `RpcGateway` and `TaskProductApi` now receive the same runtime instance from
  the stdio factory, so background Agent turns and protocol clients operate on
  the same request state.
- `TaskEventSink` opens a task-scoped `permission/request`, appends the durable
  blocked permission message, then waits on the shared runtime.
- Legacy `permission.respond` compatibility now routes through the same runtime
  reservation path, so it cannot overwrite a protocol answer.
- Removed the old local permission waiter registry and its waiter-specific tests.

## Review Fixes

- Reject options now persist `PermissionDecision::Denied`; allow options persist
  `PermissionDecision::Approved`.
- Permission requests are opened before the blocked-message commit, so the
  `task.updated` notification that follows can drain and deliver the broker
  request.
- Append failure interrupts the broker request; cancellation removes the waiter
  and interrupts the broker request so snapshots do not expose ghost pending
  rows.
- Mixed protocol/legacy answers use first-response-wins before durable commit;
  late legacy answers are rejected before they can mutate chat.
- Runtime and event-sink modules were split to keep production files below the
  project size threshold.

## Verification

- `cargo test -p openaide-runtime server_requests -- --nocapture` passed.
- `cargo test -p openaide-runtime permission -- --nocapture` passed.
- `cargo test -p openaide-runtime protocol_edge -- --nocapture` passed.
- `cargo test -p openaide-runtime` passed.

## Next

Select the next refactor slice from the top-level plan. The highest-value next
candidate is moving shared Frontend/App Server connection code toward the typed
BackendConnection and central intent layer, unless we decide to finish more
Backend request categories first.
