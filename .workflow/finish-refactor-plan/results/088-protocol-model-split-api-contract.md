# P67 Protocol Model Split API Contract

Completed: 2026-06-27T04:35:00+03:00

## Accepted Shape

Replace `openaide-rs/app-server/src/protocol/model.rs` with a module directory:

- `protocol/model/mod.rs`
- `protocol/model/task.rs`
- `protocol/model/chat.rs`
- `protocol/model/activity.rs`
- `protocol/model/permission.rs`
- `protocol/model/agent.rs`

`protocol/model/mod.rs` is the public namespace owner. It re-exports the public
protocol model records from focused private submodules so existing call sites can
continue importing `crate::protocol::model::TypeName`.

## Module Ownership

`task.rs` owns Task-facing model records:

- `TaskStatus`
- `IsolationKind`
- `TaskSummary`
- `TaskSnapshot`
- `SettingsSummary`

`chat.rs` owns Chat and normalized message records:

- `MessagePage`
- `ChatMessage`
- `NormalizedMessage`
- `Attachment`
- `InterruptionReason`
- the existing `NormalizedMessage` helper methods

`activity.rs` owns activity and tool-detail records:

- `ActivityStatus`
- `ActivityStep`
- `ActivityToolDetails`
- `ActivityToolLocation`
- `ActivityToolContent`
- `ActivityToolInput`
- `ActivityToolOutput`
- `ActivityToolField`

`permission.rs` owns permission records:

- `PermissionState`
- `PermissionToolCall`
- `PermissionOption`
- `PermissionOptionKind`
- `PermissionDecision`

`agent.rs` owns Agent-facing model records:

- `ConfigOptionsStatus`
- `ConfigOptionsCatalog`
- `ConfigOptionCategory`
- `ConfigOption`
- `ConfigOptionValue`
- `AgentProbeStatus`
- `AgentAuthMethodSummary`
- `AgentProbeResult`
- `AgentAuthenticateStatus`
- `AgentAuthenticateResult`
- `AgentListedSession`
- `AgentListSessionsResult`
- the existing `ConfigOptionsCatalog` helper methods

## Stable API

- The external Rust import path remains `crate::protocol::model::TypeName`.
- Public type names, fields, serde attributes, derives, helper method names, and
  helper method behavior stay unchanged.
- Existing protocol params/results/notifications continue depending on
  `protocol::model` instead of reaching into private submodules.
- Private submodules may import sibling model types through `super::...` only where
  one protocol record structurally contains another.

## Non-Goals

- No protocol shape changes.
- No runtime behavior changes.
- No generated TypeScript binding semantic changes.
- No App Shell contract redesign.
- No storage schema changes.
- No public module path churn through runtime call sites.
- No duplicated compatibility copies of old structs.

## Review And Test Requirements

- `protocol/model/mod.rs` must stay a small namespace file, not become the new grab
  bag.
- All touched production source files must stay below the 400-line limit.
- `cargo test -p openaide-runtime` must pass.
- `npm run check` must pass.
- `npm test` must pass.
- `cargo fmt --all --check` and `git diff --check` must pass before commit.
