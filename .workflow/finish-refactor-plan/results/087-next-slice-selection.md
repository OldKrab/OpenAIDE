# P66 Next Slice Selection

Completed: 2026-06-27T04:35:00+03:00

## Selected Slice

Split the oversized protocol render model definitions out of
`protocol/model.rs` into a focused `protocol/model/` module tree.

## Rationale

The latest source-size scan shows `openaide-rs/app-server/src/protocol/model.rs`
at 489 lines, above the production source-file limit. It is also a grab bag of
several independent protocol model categories:

- Task navigation and Task snapshot structs;
- Chat and normalized message structs;
- Activity/tool-detail structs;
- permission decision structs;
- Agent probe/auth/session-list/config-option structs.

These are all protocol-facing serializable records, so they belong together under
the `protocol::model` namespace, but not in one file. Splitting them by domain
makes the protocol model easier to review and keeps the next work aligned with the
accepted App Server Protocol boundary.

## Non-Selection

Do not redesign the protocol in this slice.

Do not move protocol params, results, notifications, JSON-RPC helpers, host
request types, runtime settings, diagnostics, storage records, or Agent ACP
runtime logic.

Do not update generated TypeScript bindings or App Shell contracts unless existing
checks require a mechanical refresh. The intended change is internal Rust module
layout only.
