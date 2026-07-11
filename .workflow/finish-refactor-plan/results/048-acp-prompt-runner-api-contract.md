# P27 ACP Prompt Runner API Contract

Completed: 2026-06-27T02:56:26+03:00

## Accepted Shape

Add a focused internal module:

- `agent/acp_prompt_runner.rs`

It owns:

- `run_prompt(...)`
- prompt request dispatch;
- active prompt cancellation notification;
- current prompt registration guard used by host capability handlers;
- prompt content building and validation for the prompt request;
- streamed prompt update projection through `LivePromptProjection`.

## Ownership

- `AcpSessionWorker` owns session startup/load, command loop, prompt command dispatch,
  session event sink/catalog delivery, delete, close, and worker lifetime.
- `AcpPromptRunner` owns only a single prompt turn once the worker command loop hands it
  the active session, cancellation channel, close channel, prompt context, prompt, and
  event sink.
- `AcpHostCapabilityHandlers` continues to receive the current prompt slot so host
  requests can be attributed to the active prompt.

## Non-Goals

- No ACP behavior change.
- No prompt cancellation behavior change.
- No close/delete lifecycle change.
- No protocol mapping change.
- No public Agent runtime API change.
- No test deletion or weakening.

## Review And Test Requirements

- Existing ACP prompt, cancellation, terminal, and close tests must keep passing.
- `cargo test -p openaide-runtime agent::acp::tests -- --nocapture` must pass.
- `cargo test -p openaide-runtime` and `npm test` must pass.
- The new prompt runner module must stay below the production source-size limit.
