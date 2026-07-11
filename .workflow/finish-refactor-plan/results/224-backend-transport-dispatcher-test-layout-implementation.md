# Backend Transport Dispatcher Test Layout Implementation

Implemented the accepted Backend Transport Dispatcher test-layout split only.

Changed modules:
- `transport/dispatch.rs` now keeps production dispatcher code plus
  `#[cfg(test)] mod tests;`.
- `transport/dispatch/tests.rs` owns dispatcher-boundary tests for invalid JSON,
  invalid JSON-RPC versions, notifications, unknown methods, host responses,
  pending host bridge responses, and shutdown rejection.

Focused verification before review:
- `cargo fmt --all`
- `cargo check -p openaide-runtime`
- `cargo test -p openaide-runtime transport::dispatch::tests -- --nocapture`

