# P51 Next Slice Selection

Completed: 2026-06-27T03:39:17+03:00

## Selected Slice

Split the ACP session client and command interface out of
`agent/acp_session_worker.rs`.

## Rationale

`agent/acp_session_worker.rs` is still oversized at 559 lines. Its top section mixes
the channel-facing session client, command/input types, startup result type, and
worker-stopped error rendering with the actual ACP session worker loop.

The cleanest next boundary is to extract the command/client surface first:

- `AcpSessionClient`;
- `AcpSessionCommand`;
- `AcpSessionWorkerInput`;
- `AcpSessionOpenRequest`;
- `AcpStartedSession`;
- terminal error recording and worker-stopped error presentation used by the client.

That split should bring `acp_session_worker.rs` below the production source-file size
limit while preserving the worker loop as the owner of ACP session startup, update
reading, prompt dispatch, close, delete, and config catalog delivery.

## Non-Selection

Do not split `acp_runtime_kernel.rs` in this slice. It remains oversized and is a
strong later candidate, but the session client extraction gives a clearer immediate
module boundary with lower behavior risk.

Do not move the ACP session worker loop, ACP client builder, host capability request
handlers, prompt execution, session start/load, config-option application, close, or
delete behavior in this slice.
