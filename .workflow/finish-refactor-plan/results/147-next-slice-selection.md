# Next Slice Selection: ACP Session Opening Split

## Decision

Select the ACP Session opening split as the next Backend refactor slice.

## Why This Slice

`agent/acp_session_worker.rs` still owns two different phases:

- opening an ACP session: initialize, capability policy derivation, prompt
  attachment validation, `session/new` or `session/load`, config-option
  application, load replay capture, started response construction, and opening
  error mapping;
- running an already opened live session: command handling, prompt dispatch,
  close/delete, session config catalog delivery, and active update reading.

Those phases have different invariants. Splitting opening gives the worker a
clear shape: connect, open the requested session, report startup, then run the
live command/update loop.

## Proposed Boundary

Create a focused `agent/acp_session_opening.rs` module for:

- initialize request tracing and `initialize_agent_connection`;
- `AcpSessionRunner` construction for the opened connection;
- prompt content policy derivation;
- start-request prompt attachment validation;
- `session/new` start flow;
- initial config-option application and close-on-apply-failure behavior;
- `session/load` flow with replay capture;
- startup error reporting through the existing `started_tx`;
- returning the opened active session, runner facts, content policy, applied
  catalog, and replayed messages needed by the worker.

Keep `agent/acp_session_worker.rs` responsible for:

- worker input contracts;
- ACP client connection call;
- command loop;
- prompt dispatch through `acp_prompt_runner`;
- close/delete command execution;
- session config catalog buffering/delivery;
- active session update reading after startup;
- final `acp_error` mapping.

## Out Of Scope

- No behavior changes to start/load, config-option application, close on
  failure, replay capture, tracing, prompt content validation, startup errors,
  command loop behavior, or session config catalog delivery.
- No changes to `AcpSessionClient`, active session manager, protocol,
  Frontend, storage, or Task workflows.

## Next Step

Grill and record the accepted API contract for this slice before implementation.
