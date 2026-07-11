# Backend Transport Dispatcher Test Layout API Contract

Accept the Backend Transport Dispatcher test-layout split.

Public API:
- Keep `transport::dispatch::Dispatcher` importable from the same path.
- Keep all dispatcher public methods and behavior unchanged.

Internal module contract:
- Replace the inline test module in `transport/dispatch.rs` with
  `#[cfg(test)] mod tests;`.
- Move existing dispatcher tests to `transport/dispatch/tests.rs`.
- Keep tests at the dispatcher boundary: instantiate `Dispatcher`, call
  `handle_line`, and assert serialized JSON-RPC responses or side effects.
- Do not add new production helpers solely to support the move.

Behavior to preserve:
- Invalid JSON parse error response with null id.
- Invalid JSON-RPC version invalid-request response with original id.
- Unknown notifications produce no response and log `rpc_notification_failed`.
- Unknown methods return `method_not_found`.
- Host responses are consumed before request dispatch.
- Malformed host response shapes still fall through to invalid-request handling.
- Pending host bridge requests are unblocked by host responses.
- Requests after shutdown are rejected without mutating storage.

Review focus:
- Ensure tests still cover the same public dispatcher boundary.
- Ensure the move does not accidentally widen production visibility.
- Ensure `dispatch.rs` keeps only production dispatcher code plus the external
  test-module declaration.

