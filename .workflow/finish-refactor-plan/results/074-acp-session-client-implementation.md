# P53 ACP Session Client Implementation

Completed: 2026-06-27T03:44:30+03:00

## Implemented

- Added `agent/acp_session_client.rs`.
- Moved the synchronous caller-facing ACP session handle out of
  `agent/acp_session_worker.rs`.
- Moved ACP session command envelopes, worker input, open request, started-session
  result, and stopped-worker terminal error presentation into the new module.
- Registered the new module in `agent/mod.rs`.

## Ownership

- `acp_session_client.rs` owns `AcpSessionClient`, `AcpSessionCommand`,
  `AcpSessionWorkerInput`, `AcpSessionOpenRequest`, `AcpStartedSession`, and
  stopped-worker error rendering.
- `acp_session_worker.rs` owns the live ACP worker loop, ACP `Client` builder,
  session start/load, prompt dispatch, active update reading, close/delete handling,
  and config catalog delivery.
- `acp_runtime_kernel.rs` owns session registry, thread spawning, duplicate active
  session protection, and runtime-level session lifecycle entry points.

## Behavior

No intended behavior change. Command channel behavior, prompt/close/delete timeouts,
worker-stopped error text, session start/load/close/delete request ordering, config
catalog buffering, and public Agent runtime APIs are unchanged.
