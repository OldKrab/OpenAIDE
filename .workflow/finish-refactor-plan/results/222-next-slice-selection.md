# Next Slice Selection: Backend Transport Dispatcher Test Layout Split

Select the Backend Transport Dispatcher test-layout split as the next refactor
slice.

Reasoning:
- `transport/dispatch.rs` still contains a large inline `#[cfg(test)] mod tests`
  block after the dispatcher split.
- Project rules require Rust tests to live in separate test files where
  practical, and the backend already uses sibling `tests.rs` modules for
  `server_requests`, `state_sync`, `client_lifecycle`, `app_lifecycle`, and
  other modules.
- Moving tests out of the production module makes the `Dispatcher` facade easier
  to read and keeps the previous dispatcher behavior coverage intact.

Out of scope:
- No production behavior changes.
- No JSON-RPC, shutdown, host bridge, or method routing changes.
- No test semantic changes beyond any import adjustments required by the move.

