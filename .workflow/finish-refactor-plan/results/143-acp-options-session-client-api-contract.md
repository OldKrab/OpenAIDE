# ACP Options Session Client Split: API Contract

## Accepted Contract

Split the ACP options-session command client and command channel types out of
`agent/acp_options_session.rs` without changing prepared options-session
behavior.

## Caller-Facing Surface

Create `agent/acp_options_session_client.rs` for the channel-facing API:

- `AcpOptionsSessionClient`;
- `AcpOptionsCommandReceiver`;
- `AcpOptionsCommand`;
- `options_session_channel`.

`AcpOptionsSessionManager` imports the client and channel constructor from the
new module. `AcpOptionsSessionWorkerInput` receives the command receiver from
the new module. No other caller should construct options commands directly.

## Client Module Responsibilities

`acp_options_session_client.rs` owns:

- synchronous client methods: `config_options`, `set_config_option`,
  `list_sessions`, and `close`;
- command channel construction;
- request/reply channel setup;
- stopped-worker error mapping for send/receive failures;
- existing `list_sessions` timeout behavior;
- existing `close` timeout behavior;
- the command enum shape consumed by the worker.

It must not own:

- ACP connection setup;
- ACP `Client` builder wiring;
- permission invalidation behavior;
- catalog projection;
- set-option application;
- list-session ACP request execution;
- close execution against `AcpSessionRunner`;
- prepared options-session reuse or retry policy.

## Worker Module Responsibilities

`acp_options_session.rs` keeps:

- `AcpOptionsSessionWorkerInput`;
- `run_options_session`;
- ACP agent construction and initialize;
- prepared options-session startup;
- permission-request invalidation;
- catalog projection and updates;
- command loop execution;
- `AcpOptionsCommand` matching;
- `AcpSessionRunner` list-session and close calls;
- error mapping through `acp_error`.

## Behavior Invariants

This slice must preserve:

- `config_options` and `set_config_option` blocking receive behavior and stopped
  worker errors;
- `list_sessions` 15-second timeout behavior and error message;
- `close` 2-second timeout behavior and error message;
- command payloads and reply types;
- prepared options-session startup, retry, and invalidation behavior;
- permission-request invalidation message and cancellation response;
- catalog update behavior after set-option and session updates;
- list-session request dispatch behavior;
- shutdown close behavior through the manager.

## Visibility

Keep all new exports `pub(super)`. The command enum may be `pub(super)` because
the worker must match it, but command construction remains encapsulated in
client methods outside tests.

## Out Of Scope

- No async/channel redesign.
- No timeout changes.
- No manager retry or lifecycle policy changes.
- No changes to ACP session request semantics.
- No protocol, Frontend, storage, or Task workflow changes.

## Review Requirements

`$doomsday-review` must check at least:

- no command execution logic moved into the client module;
- worker behavior remains equivalent;
- stopped-worker and timeout errors remain equivalent;
- `AcpOptionsSessionManager` policy is unchanged except imports;
- visibility does not expose command construction wider than before.

## Verification Plan

Run focused options-session tests:

- `cargo test -p openaide-runtime options_session_update -- --nocapture`
- `cargo test -p openaide-runtime options_connection_list_excludes_prepared_session -- --nocapture`
- `cargo test -p openaide-runtime options_start_failure_reports_agent_error_instead_of_closed_start_channel -- --nocapture`

Then run:

- `cargo test -p openaide-runtime`
- `cargo fmt --all --check`
- `npm run check`
- `git diff --check`
