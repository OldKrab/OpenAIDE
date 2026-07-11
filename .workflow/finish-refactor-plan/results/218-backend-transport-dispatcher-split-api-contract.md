# Backend Transport Dispatcher Split API Contract

Accept the Backend Transport Dispatcher split.

Public API:
- Keep `transport::dispatch::Dispatcher` importable from the same path.
- Keep `Dispatcher::new`, `Dispatcher::new_with_host`, `handle_line`, and
  `shutdown_requested` behavior unchanged.
- Keep response serialization format and order unchanged for single requests,
  notifications, host responses, parse failures, invalid requests, and batches.

Internal module contract:
- Extract JSON-RPC line/value handling helpers from method routing where it
  improves isolation, but `Dispatcher` remains the facade that owns `Runtime`,
  `HostBridge`, and shutdown state.
- Extract method routing into a focused module or helper that receives explicit
  runtime/shutdown dependencies and returns the same `RuntimeError`/`Value`
  outcomes.
- Keep host-response consumption before JSON-RPC request deserialization.
- Keep parse, `to_value`, and response serialization helpers transport-local;
  do not turn them into project-wide abstractions in this slice.
- Keep parameter type-check scaffolding transport-local unless the split makes a
  narrower private module a better owner.

Behavior to preserve:
- Invalid JSON returns one parse-error response with id `null`.
- Array batches are processed in input order, with host responses producing no
  outgoing response.
- Malformed host response shapes that are not accepted by `HostBridge` still
  fall through to normal invalid-request handling.
- Non-`2.0` JSON-RPC requests return invalid request errors with the original
  request id.
- Notifications never produce responses and log dispatch failures.
- After `runtime.shutdown`, later non-shutdown requests in the same or later
  batch are rejected as `not_ready` without mutating storage.
- Unknown methods still return `method_not_found`.

Review focus:
- Ensure splitting does not change request/response order in batches.
- Ensure shutdown state is mutated only after successful runtime shutdown.
- Ensure host-response handling still precedes request validation.
- Ensure method routing helpers do not hide runtime ownership or create
  singleton state.
- Keep tests at the transport boundary and add focused coverage only if a moved
  invariant is no longer directly protected.

