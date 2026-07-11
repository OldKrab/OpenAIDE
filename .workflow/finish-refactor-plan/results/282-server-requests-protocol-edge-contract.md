# A4a Server Requests Protocol Edge Contract

Goal: make `server_requests` an active App Server Protocol edge component before
wiring Task permission prompts to it.

## Scope

- `RpcGateway` owns a shared `ServerRequestBroker`.
- `client/initialize`, `state/subscribe`, `state/unsubscribe`, transport close,
  and task-update publication notify the broker about responder availability.
- Broker deliveries are emitted as Backend-initiated requests to the target
  protocol connection.
- Client responses to Backend-initiated requests are parsed by the protocol edge
  and routed through `ServerRequestBroker::handle_response`.
- Task and client snapshots include broker-owned pending request rows.

## Non-Scope

- Do not yet replace `TaskEventSink` permission waiters.
- Do not design shell/secret request methods beyond preserving broker support.
- Do not change ACP HostBridge behavior in this slice.

## API Decisions

- Stdio wire uses JSON-RPC request messages for Backend-initiated requests:
  `{ "jsonrpc": "2.0", "id": "<requestId>", "method": "...", "params": ... }`.
- Client answers are JSON-RPC responses with matching `id`; successful `result`
  is accepted by the broker, malformed/failed answers remain rejected or
  unresolved according to broker lifecycle.
- Broker response handling is side-effect-only for the protocol edge in this
  slice; it returns no client-visible acknowledgement for a response message.

## Verification

- Protocol-edge tests cover:
  - initialized/subscribed clients receive broker deliveries;
  - task snapshots include pending task request rows;
  - a client response resolves the broker and removes pending rows;
  - stale/unauthorized responses do not resolve.
- Review found and this slice fixed:
  - HostBridge JSON-RPC responses are not intercepted by server-request routing;
  - JSON-RPC error answers remain invalid and do not resolve pending requests;
  - accepted task-scoped answers publish a task snapshot replacement so pending
    rows disappear without waiting for unrelated updates;
  - reattached clients are observed with existing task subscription scopes;
  - pending request snapshot rows are sorted deterministically.
- `cargo test -p openaide-runtime protocol_edge` passed.
- `cargo test -p openaide-runtime` passed.
- Production source-size scan after the split:
  - `protocol_edge.rs`: 288 lines;
  - `protocol_edge/server_request_handlers.rs`: 259 lines;
  - `protocol_edge/stdio.rs`: 254 lines.

## Next

A4b wires Agent permission requests to the broker-backed path and removes local
permission waiters.
