# Next Slice Selection: ACP Options Session Client Split

## Decision

Select the ACP Options Session client split as the next Backend refactor slice.

## Why This Slice

`agent/acp_options_session.rs` currently mixes:

- the synchronous channel-facing `AcpOptionsSessionClient` API used by the
  options-session manager;
- the command enum and command receiver;
- the async live ACP options worker loop;
- ACP options session startup, invalidation, update handling, list-session
  request dispatch, close behavior, and worker error mapping.

The command client and worker loop have different callers and different
failure surfaces. Splitting the client interface into its own module matches the
existing live task-session split (`acp_session_client` plus
`acp_session_worker`) and gives the options worker a smaller responsibility.

## Proposed Boundary

Create a focused `agent/acp_options_session_client.rs` module for:

- `AcpOptionsSessionClient`;
- command channel construction;
- command receiver wrapper;
- command enum shared with the worker;
- stopped-worker and timeout error mapping for client methods.

Keep `agent/acp_options_session.rs` responsible for:

- `AcpOptionsSessionWorkerInput`;
- live ACP options worker startup;
- ACP `Client` wiring for prepared options sessions;
- permission-request invalidation behavior;
- command loop execution;
- catalog projection and update handling;
- list-session and close dispatch through `AcpSessionRunner`.

## Out Of Scope

- No behavior changes to prepared options session reuse, retry, invalidation,
  close, list-session timeout, set-option updates, startup errors, or ACP
  event handling.
- No changes to `AcpOptionsSessionManager` policy beyond import paths.
- No changes to protocol, Frontend, storage, or Task workflows.
- No broad ACP options-session lifecycle redesign.

## Next Step

Grill and record the accepted API contract for this slice before implementation.
