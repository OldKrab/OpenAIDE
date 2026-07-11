# P26 Next Slice Selection

Completed: 2026-06-27T02:56:26+03:00

## Selected Slice

Extract ACP prompt execution helpers out of `agent/acp_session_worker.rs`.

## Why This Slice

- `agent/acp_session_worker.rs` is the largest remaining production file.
- The prompt execution block is cohesive and distinct from session startup and the
  worker command loop.
- Extracting it improves module boundaries before any deeper ACP session lifecycle
  changes.

## Scope

- Move prompt execution helpers to a focused Agent module.
- Keep `AcpSessionWorker` as owner of session startup, command dispatch, delete, event
  sink/catalog delivery, and worker lifetime.
- Preserve prompt cancellation, close-while-prompt behavior, trace recording, and event
  projection.
- Do not change ACP protocol mapping or public Agent runtime APIs.

## Main Risk

Prompt execution is cancellation-sensitive. The implementation must preserve the
existing `tokio::select!` behavior for cancellation, close requests, prompt result
delivery, and streamed session updates.
