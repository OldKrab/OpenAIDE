# ACP Session Opening Split: API Contract

## Accepted Contract

Split ACP session opening out of `agent/acp_session_worker.rs` without changing
start/load behavior or the live worker command loop.

## Caller-Facing Surface

Create `agent/acp_session_opening.rs` for the worker-internal opening phase.

It exposes a single worker-facing function:

- `open_acp_session(...) -> Result<OpenedAcpSession, agent_client_protocol::Error>`

The exact function signature may take a small context struct if that keeps
parameters readable. It must be called only from `run_acp_session` after
`connect_acp_session_client` yields a connection.

## Opening Module Responsibilities

`acp_session_opening.rs` owns:

- initialize request creation and trace recording;
- `initialize_agent_connection`;
- `AcpSessionRunner` construction;
- `supports_session_close` discovery;
- prompt content policy derivation from initialize capabilities;
- start-request prompt attachment validation;
- `session/new` start flow;
- initial config-option application for start flow;
- closing the newly started session if config-option application fails;
- `session/load` flow with replay capture;
- startup error reporting through the existing `started_tx`;
- construction of the opened-session result.

`OpenedAcpSession` returns:

- active session handle;
- `AcpSessionRunner`;
- `supports_session_close`;
- `PromptContentPolicy`;
- `AgentSession` for startup reporting;
- replayed messages.

## Worker Module Responsibilities

`acp_session_worker.rs` keeps:

- `AcpSessionWorkerInput`;
- `AcpSessionOpenRequest`;
- `AcpStartedSession`;
- ACP client connection call;
- `started_tx` success send after opening;
- live command loop;
- prompt dispatch through `run_prompt`;
- close/delete command handling;
- session event sink buffering and delivery;
- active session update reading after startup;
- final `.await.map_err(acp_error)` mapping.

The worker may continue to own small event-sink helper functions unless review
finds a stronger boundary.

## Behavior Invariants

This slice must preserve:

- initialize request trace event name and payload;
- initialization failure behavior and startup error reporting;
- start flow `session/new` cwd normalization and ACP error formatting;
- prompt attachment validation timing before `session/new` start result is
  reported;
- config-option application behavior and close-on-apply-failure behavior;
- load flow cwd normalization, replay capture, and ACP start-error mapping;
- `AgentSession::with_config_options` result for startup success;
- replayed messages in `AcpStartedSession`;
- `supports_session_close` value used by prompt runner;
- prompt content policy used by prompt runner;
- command loop behavior after opening.

## Visibility

Keep opening types and functions `pub(super)`. Do not expose them outside Agent
ACP internals.

## Out Of Scope

- No command loop redesign.
- No changes to `AcpSessionClient`, active-session manager, prompt runner,
  session runner, protocol, storage, Frontend, or Task workflows.
- No new tests required unless the split exposes an uncovered behavior risk.

## Review Requirements

`$doomsday-review` must check at least:

- startup success/failure reporting still happens exactly once;
- start/load error strings and mapping are unchanged;
- config-option failure still closes the newly started session;
- worker command loop still receives the same runner facts and content policy;
- opening module does not own live command execution.

## Verification Plan

Run focused session startup tests:

- `cargo test -p openaide-runtime active_session_runtime::start_prompt_and_close_dispatch_through_active_sessions -- --nocapture`
- `cargo test -p openaide-runtime active_session_runtime::load_session_registers_active_session_for_close -- --nocapture`
- `cargo test -p openaide-runtime options_start_failure_reports_agent_error_instead_of_closed_start_channel -- --nocapture`
- `cargo test -p openaide-runtime prompt_content_includes_text_and_resource_links_for_path_attachments -- --nocapture`

Then run:

- `cargo test -p openaide-runtime`
- `cargo fmt --all --check`
- `npm run check`
- `git diff --check`
