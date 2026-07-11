# Next Slice Selection: Frontend Host Message Router Split

## Selected Slice

Split `packages/frontend/src/state/hostMessageRouter.ts` into focused domain routers while keeping `routeHostMessage` as the single public ingress for shell/webview messages.

## Why This Slice

- `hostMessageRouter.ts` is the current Frontend ingestion boundary for App Shell messages.
- It mixes settings, Agent options, navigation, Task snapshots, chat paging, tool details, attachment results, and runtime error routing in one module.
- This boundary is architecturally important because direct shell/protocol access must stay centralized; render components and reducers should not learn transport message details.
- The file is still under the hard source-size limit, but its responsibilities are broader than its name-level API.
- Existing tests already exercise representative routing behavior, so a focused split can preserve behavior with targeted coverage.

## Scope

In scope:

- Keep `routeHostMessage(message, context)` as the public API used by `appController.ts`.
- Keep `sendWebviewTelemetry` importable from `hostMessageRouter.ts` unless the accepted API contract later chooses a narrower telemetry helper export.
- Extract focused modules for settings/catalog/preference routing, Agent option/native-session routing, navigation routing, Task/chat/tool routing, and runtime error routing.
- Preserve stale request filtering for config options, native sessions, Task snapshots, archive list mode, and telemetry for accepted/ignored snapshots.
- Preserve all dispatch actions, posted shell messages, fallback error messages, and callback timing.
- Keep the split shell-neutral inside Frontend state modules; no rendering component should import domain router internals.

Out of scope:

- No App Server Protocol redesign.
- No reducer action renames.
- No host bridge message contract changes.
- No App Shell or VS Code extension changes.
- No behavior changes to navigation, Task opening, settings refresh, native session pagination, or runtime error presentation.

## Review Risks

- Stale-response guards can move to the wrong domain or be dropped.
- Runtime error routing is easy to over-generalize and can dispatch the wrong user-facing error.
- Snapshot telemetry and navigation follow-up actions can change ordering.
- Extracted modules could accidentally expose transport details outside the central router boundary.

## Proposed Next Packet

Grill and accept the API contract for the Frontend Host Message Router split before implementation.
