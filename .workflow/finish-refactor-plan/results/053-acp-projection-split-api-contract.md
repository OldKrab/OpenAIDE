# P32 ACP Projection Split API Contract

Completed: 2026-06-27T03:07:57+03:00

## Accepted Shape

Split `agent/acp_update_projection.rs` into focused internal projection modules while
preserving caller-facing type names and behavior:

- `agent/acp_live_prompt_projection.rs`
- `agent/acp_replay_projection.rs`
- `agent/acp_config_projection.rs`

Keep `agent/acp_update_projection.rs` as a thin compatibility module only if it helps
the slice stay mechanical. It may re-export the stable internal projection types for
existing callers during this slice, but it must not remain an oversized implementation
bucket.

## Stable Projection API

The slice preserves these caller-facing APIs:

- `LivePromptProjection::new(agent_id, sink, cancellation)`
- `LivePromptProjection::cancellation()`
- `LivePromptProjection::permission_response(request)`
- `LivePromptProjection::emit(update)`
- `ReplayProjection::new(agent_id).project(updates)`
- `PreparedOptionsProjection::new(agent_id, notifier, options_request_key)`
- `PreparedOptionsProjection::catalog(options)`
- `PreparedOptionsProjection::apply_dispatch(dispatch, catalog, notify_config_updates)`
- `SessionConfigProjection::new(agent_id).catalog_from_dispatch(dispatch)`
- `normalize_config_options(agent_id, options)`

## Ownership

- `AcpLivePromptProjection` owns live prompt event projection, active tool-call merge
  state for live prompts, and ACP permission-request conversion.
- `AcpReplayProjection` owns replayed ACP session update normalization into durable
  normalized Chat messages.
- `AcpConfigProjection` owns ACP config-option normalization, prepared-options update
  handling, and active-session config catalog extraction.

If live and replay projection need identical ACP tool-call merge behavior, extract a
small private helper module for that behavior instead of duplicating merge logic.

## Non-Goals

- No ACP behavior change.
- No Agent event mapping change.
- No ConfigOptionsCatalog shape change.
- No permission request/response behavior change.
- No replay history behavior change.
- No public Agent runtime API change.
- No test deletion or weakening.

## Review And Test Requirements

- Existing ACP projection, options, permission, and replay tests must keep passing.
- `cargo test -p openaide-runtime agent::acp::tests -- --nocapture` must pass.
- `cargo test -p openaide-runtime` and `npm test` must pass.
- New production source files must stay below the source-size limit.
