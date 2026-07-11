# Protocol Model Split Implementation

## Scope

Implemented the accepted contract in `088-protocol-model-split-api-contract.md`.

## Code Changes

- Replaced the oversized `protocol/model.rs` file with a `protocol/model/`
  module tree.
- Added `protocol/model/mod.rs` as the stable namespace and re-export owner.
- Added focused model files:
  - `task.rs` for Task summary/snapshot/settings records.
  - `chat.rs` for Chat, normalized message, attachment, and interruption records.
  - `activity.rs` for activity and tool-detail records.
  - `permission.rs` for permission records.
  - `agent.rs` for Agent probe/auth/session-list/config-option records.

## Stable API

- Existing call sites continue importing types through `crate::protocol::model::*`.
- Public type names, field names, serde attributes, derives, and helper methods were
  preserved.
- No protocol params, results, notifications, storage records, generated TypeScript
  bindings, or runtime call sites were intentionally changed.

## Source Size

- `protocol/model/mod.rs`: 20 lines
- `protocol/model/activity.rs`: 118 lines
- `protocol/model/agent.rs`: 132 lines
- `protocol/model/chat.rs`: 151 lines
- `protocol/model/permission.rs`: 42 lines
- `protocol/model/task.rs`: 58 lines

The previous 489-line production file no longer exists, and all replacement files
are below the 400-line production source-file limit.
