# Next Slice Selection

Date: 2026-06-27

## Selected Slice

Split ACP client/host wiring out of `agent/acp_session_worker.rs`.

## Why This Slice

- `agent/acp_session_worker.rs` is the largest remaining production Rust source file at 386 lines, close to the 400-line cap.
- The file currently mixes four concerns:
  - worker entry contracts and start/load request records;
  - ACP `Client::builder()` setup;
  - host capability request wiring;
  - live session start/load and command-loop handling.
- The ACP client/host wiring block is mechanically extractable and has a clear boundary: it configures how ACP notifications and Agent-initiated host requests enter OpenAIDE, but it should not own session opening or command-loop behavior.

## Proposed Contract To Grill Next

Create a focused module such as `agent/acp_session_client_builder.rs` or a better name chosen during grilling.

It should own:

- constructing the ACP `Client` builder used by active sessions;
- registering `session/update` notification interception for load replay and trace recording;
- registering Agent-initiated host capability request handlers:
  - `session/request_permission`;
  - `fs/read_text_file`;
  - `fs/write_text_file`;
  - `terminal/create`;
  - `terminal/output`;
  - `terminal/wait_for_exit`;
  - `terminal/kill`;
  - `terminal/release`;
- preserving host capability behavior, trace recording, load replay capture, and notification fallback behavior exactly.

`agent/acp_session_worker.rs` should keep:

- worker input/start/load result contracts;
- `run_acp_session`;
- session opening through `AcpSessionRunner`;
- prompt/cancel/close/delete command loop;
- session config catalog buffering and delivery helpers unless the API grill chooses a separate tiny split.

## Non-Goals

- No behavior changes to ACP initialize, start, load, prompt, cancel, close, delete, host bridge, terminal, filesystem, permissions, tracing, or replay.
- No changes to public runtime APIs.
- No changes to ACP protocol schema or generated bindings.
- No new abstractions for options sessions.

## Initial Verification Plan

- `cargo test -p openaide-runtime agent::acp::tests -- --nocapture`
- `cargo test -p openaide-runtime`
- `npm run check`
- `npm test`
- `cargo fmt --all --check`
- `git diff --check`
- production source-size scan.
