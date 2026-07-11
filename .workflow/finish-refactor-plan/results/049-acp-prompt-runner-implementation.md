# P28 ACP Prompt Runner Implementation

Completed: 2026-06-27T03:06:00+03:00

## Implemented

- Added `agent/acp_prompt_runner.rs`.
- Moved prompt-turn execution out of `agent/acp_session_worker.rs`.
- Kept `AcpSessionWorker` as the owner of session startup/load, command dispatch,
  close/delete, session event sink/catalog delivery, and worker lifetime.
- Kept prompt content build/validation for `session/prompt` inside the prompt runner.
- Removed prompt content policy from `AcpSessionClient` and `AcpStartedSession`, so
  the session client no longer owns prompt-shape validation.

## Prompt Runner Boundary

`AcpPromptRunner` now owns:

- ACP prompt request construction and dispatch;
- prompt response tracing;
- active prompt cancellation notification and tracing;
- close-while-prompt handling through the session lifecycle helper;
- current prompt registration for host capability attribution;
- streamed prompt update projection through `LivePromptProjection`.

## Behavior

No intended ACP behavior change. The extraction preserves prompt cancellation,
close handling, trace recording, prompt response handling, and streamed Agent event
projection.
