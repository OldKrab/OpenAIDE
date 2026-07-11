# P33 ACP Projection Split Implementation

Completed: 2026-06-27T03:13:58+03:00

## Implemented

- Split `agent/acp_update_projection.rs` into focused internal projection modules:
  - `agent/acp_live_prompt_projection.rs`
  - `agent/acp_replay_projection.rs`
  - `agent/acp_config_projection.rs`
  - `agent/acp_tool_call_projection.rs`
- Kept `agent/acp_update_projection.rs` as a thin re-export layer for stable callers.
- Kept caller-facing projection type names and method signatures stable.

## Ownership

- `AcpLivePromptProjection` owns live prompt event projection, permission-request
  conversion, and live tool-call merge state.
- `AcpReplayProjection` owns replayed ACP session updates into durable normalized Chat
  messages.
- `AcpConfigProjection` owns config-option normalization, prepared-options update
  handling, and active-session config catalog extraction.
- `AcpToolCallProjection` owns shared ACP tool-call update merging for live and replay
  projection paths.

## Behavior

No intended behavior change. The split preserves Agent event mapping, permission
response behavior, replay message normalization, config catalog shape, and ACP session
lifecycle.
