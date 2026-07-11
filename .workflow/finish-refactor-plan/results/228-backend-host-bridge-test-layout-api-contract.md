# Backend Host Bridge Test Layout API Contract

Accept the Backend Host Bridge test-layout split.

Public API:
- Keep `protocol::host::HostBridge` and `HostRequest` importable from the same
  paths.
- Keep `HostBridge::disabled`, `channel`, `channel_with_timeout`, `is_enabled`,
  `request`, `request_with_timeout`, `request_until`, and
  `try_handle_response` behavior unchanged.

Internal module contract:
- Replace the inline test module in `protocol/host.rs` with
  `#[cfg(test)] mod tests;`.
- Move existing host bridge tests to `protocol/host/tests.rs`.
- Keep tests at the public host bridge boundary: construct `HostBridge`, make
  requests, feed responses, and assert returned values or errors.
- Do not add production helpers or widen visibility solely for the test move.

Behavior to preserve:
- Host requests serialize JSON-RPC id, method, and params and accept matching
  responses.
- Disabled host bridges reject requests with capability-missing errors.
- `request_until` can wait without the default timeout and can be cancelled.

Review focus:
- Ensure the moved tests still run under the same module path.
- Ensure `host.rs` keeps only production code plus the external test-module
  declaration.
- Ensure no host bridge runtime behavior or visibility changes.

