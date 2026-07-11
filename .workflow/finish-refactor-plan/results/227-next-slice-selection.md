# Next Slice Selection: Backend Host Bridge Test Layout Split

Select the Backend Host Bridge test-layout split as the next refactor slice.

Reasoning:
- `protocol/host.rs` still contains inline Rust tests despite the project rule
  that Rust tests should live in separate files where practical.
- `HostBridge` is a protocol-boundary module; keeping production code separate
  from tests improves readability without changing behavior.
- The existing tests already exercise the relevant public boundary and can move
  unchanged except for import adjustments.

Out of scope:
- No production behavior changes.
- No host bridge timeout, cancellation, pending-response, or error mapping
  changes.
- No JSON-RPC envelope changes.
- No visibility widening solely to support tests.

