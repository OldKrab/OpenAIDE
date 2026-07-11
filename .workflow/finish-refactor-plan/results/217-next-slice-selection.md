# Next Slice Selection: Backend Transport Dispatcher Split

Select the Backend Transport Dispatcher split as the next refactor slice.

Reasoning:
- `openaide-rs/app-server/src/transport/dispatch.rs` is the largest non-test
  production Rust transport module and mixes JSON line parsing, batch handling,
  host-response routing, request validation, shutdown gating, method dispatch,
  serialization helpers, type-check scaffolding, and tests.
- The transport dispatcher sits on the App Server protocol boundary, so a
  focused split improves reviewability of a high-risk boundary without changing
  public transport behavior.
- Existing tests already cover host-response priority, invalid request handling,
  pending host bridge response unblocking, and shutdown gating; the slice can
  preserve those while making responsibilities explicit.

Out of scope:
- No JSON-RPC protocol behavior changes.
- No method name, params, result, or error shape changes.
- No runtime service ownership changes.
- No shutdown lifecycle changes beyond moving existing logic behind the same
  behavior.
- No async transport or multi-client lifecycle changes.

