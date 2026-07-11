# ACP Session Request I/O Split API Contract

## Decision

Create `agent/acp_session_requests.rs` as the raw ACP session request I/O
module.

The module is internal to `agent` and exists to keep ACP request construction,
trace recording, and auth-required retry behavior out of session lifecycle
orchestration.

## Module API

`agent/acp_session_requests.rs` owns these worker/lifecycle-facing functions:

```rust
pub(super) async fn request_new_session(
    connection: &ConnectionTo<Agent>,
    cwd: PathBuf,
    initialize: &InitializeResponse,
    preferred_auth_method_id: Option<&str>,
    trace: Option<&AcpTraceSession>,
) -> Result<NewSessionResponse, agent_client_protocol::Error>;

pub(super) async fn request_load_session(
    connection: &ConnectionTo<Agent>,
    session_id: SessionId,
    cwd: PathBuf,
    initialize: &InitializeResponse,
    preferred_auth_method_id: Option<&str>,
    trace: Option<&AcpTraceSession>,
) -> Result<LoadSessionResponse, agent_client_protocol::Error>;

pub(super) async fn request_session_list(
    connection: &ConnectionTo<Agent>,
    cwd: PathBuf,
    cursor: Option<String>,
    initialize: &InitializeResponse,
    preferred_auth_method_id: Option<&str>,
) -> Result<ListSessionsResponse, agent_client_protocol::Error>;
```

Private helpers inside that module may include:

- `send_new_session_request`;
- `send_load_session_request`;
- `send_session_list_request`;
- a small retry helper if it reduces duplication without hiding trace behavior.

## Ownership

`agent/acp_session_requests.rs` owns:

- ACP request construction for `session/new`, `session/load`, and
  `session/list`;
- `.send_request(...).block_task().await` calls for those requests;
- AuthRequired retry for those requests, including `AuthenticateRequest`;
- trace recording for:
  - `session/new.request`;
  - `session/new.response`;
  - `session/load.request`;
  - `session/load.response`.

`agent/acp_session_lifecycle.rs` keeps:

- `LoadReplayCapture`;
- `start_active_session`;
- `LoadActiveSessionRequest`;
- `load_active_session`;
- `list_sessions_from_options_connection`;
- `close_active_session`;
- `delete_active_session`;
- `agent_list_sessions_result_from_response`;
- capability validation calls;
- active session attachment;
- load-replay capture setup and teardown;
- replay projection and config-option normalization;
- session-list filtering and normalized result construction.

## Behavior Rules

- No behavior changes.
- `session/new` keeps its existing AuthRequired retry behavior, but the retry
  moves from lifecycle code into the request module.
- `session/load` and `session/list` retry only once after successful
  authentication, as they do now.
- If `auth_method_for_session_retry` returns `None`, the original
  AuthRequired error is returned unchanged.
- The request module returns raw `agent_client_protocol::Error`; lifecycle code
  remains responsible for mapping to `RuntimeError` with `acp_error` where it
  does today.
- Trace event names, directions, and payload shapes are unchanged.
- `session/list` remains untraced in this slice, matching current behavior.
- The module must not attach sessions, project replayed messages, normalize
  config options, filter listed sessions, or map protocol errors to product
  errors.

## Tests

Existing ACP integration tests remain the main behavioral guard:

- `start_active_session_retries_auth_required_on_same_connection`;
- `session_list_retries_auth_required_on_same_connection`;
- `load_active_session_captures_replayed_updates_before_response`;
- `options_connection_list_excludes_prepared_session`.

Implementation may add focused tests only if review finds coverage weaker after
the move. The default expectation is behavior-preserving extraction with these
tests still passing.

## Implementation Scope

Allowed:

- add `agent/acp_session_requests.rs`;
- update `agent/mod.rs`;
- update imports and internal calls in `agent/acp_session_lifecycle.rs`;
- keep `request_session_list` re-exported from lifecycle only if current tests or
  sibling modules need that stable path.

Not allowed in this slice:

- move close/delete helpers;
- move replay capture or replay projection;
- change lifecycle public signatures used by `acp_session_runner.rs` or
  existing ACP tests;
- change trace naming;
- change auth retry count or selected auth method behavior;
- introduce new product errors or user-facing text.

## Acceptance Check

After implementation, `agent/acp_session_lifecycle.rs` should read as lifecycle
orchestration over request helpers, while `agent/acp_session_requests.rs` should
read as raw ACP request I/O with retry and tracing only.
