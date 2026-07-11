# Frontend Host Message Router Split API Contract

## Decision

Accept the Frontend Host Message Router split.

The external seam remains:

```ts
routeHostMessage(message: HostToWebviewMessage, context: HostMessageRouterContext): void
```

`appController.ts` remains the only caller. Rendering components, reducers, App Shells, and services must not import the new domain router internals.

## Public Exports

`hostMessageRouter.ts` keeps exporting:

- `HostMessageRouterContext`
- `routeHostMessage`
- `sendWebviewTelemetry`

No new public domain-router exports are added for app code. Internal domain router functions may be exported only if tests require it, but the preferred test surface is still `routeHostMessage`.

## Internal Modules

Create focused private modules under `packages/frontend/src/state/`:

- `hostSettingsMessages.ts`
  - Workspace root results.
  - Settings snapshot/runtime settings/preference results.
  - Agent catalog and custom Agent save/delete acknowledgements.
  - Agent authentication success and `showSettings`.
  - Preference normalization.

- `hostAgentSessionMessages.ts`
  - Agent config options result stale-key filtering.
  - Native Agent session list result stale-request filtering.
  - Native session pagination follow-up request for navigation.

- `hostNavigationMessages.ts`
  - Task list result stale archive-mode filtering.
  - Task list refresh.
  - New Task surface open request.

- `hostTaskMessages.ts`
  - Context file result routing to new-task or task composer state.
  - Task snapshot request acceptance, ignored/accepted telemetry, dispatch, navigation list refresh, and open-task surface request.
  - Chat page result.
  - Tool detail result.
  - Task refresh.

- `hostRuntimeErrorMessages.ts`
  - Runtime error routing for permission responses, tool details, chat paging, task open/refresh, task input, Agent options, settings, native sessions, and generic submit errors.
  - Settings error action classification.

If implementation shows one module is too thin or too coupled, it may be merged with an adjacent module during implementation, but only if the public seam and ownership above stay intact.

## Routing Order

`routeHostMessage` keeps the current order:

1. Settings/catalog/preference messages.
2. Agent options/native session messages.
3. Navigation messages.
4. Task/chat/tool messages.
5. Runtime error messages.

The order matters because some message type names are broad product categories and future additions should resolve deterministically.

## Behavior To Preserve

- Unknown non-error messages are ignored.
- Runtime errors are handled only by the runtime error router.
- Preference results normalize `composer_submit_shortcut` to `"enter"` or `"mod_enter"`.
- `agent.configOptions.result` and related errors ignore stale `options_request_key` values.
- `agent.listSessions.result` and related errors ignore stale request ids.
- Navigation native session pagination increments the request id, stores it as latest, and posts the next append request with the last selected Agent/workspace pair.
- `task.list.result` ignores results whose archive mode no longer matches `SnapshotRequestTracker.currentArchived()`.
- `task.snapshot` uses `SnapshotRequestTracker.accept`, sends `snapshot_ignored` telemetry for rejected snapshots, sends `snapshot_accepted` telemetry for accepted snapshots, dispatches the snapshot with the original intent, refreshes navigation task lists when on navigation surface, and opens the task surface for navigation `open` snapshots.
- `context.file.result` routes attachments by `payload.task_id`: task composer if present, new-task composer otherwise.
- Runtime error fallback messages and dispatch action shapes stay unchanged.
- `sendWebviewTelemetry` payload shape stays unchanged.

## Boundary Rules

- Domain routers may depend on state types, reducer action types, `SnapshotRequestTracker`, and shell message posting helpers.
- Domain routers must not import React components, App Shell implementations, `hostBridge`, App Server client bindings, rendering modules, or settings UI modules.
- `postHostMessage` remains injected through `HostMessageRouterContext`; domain routers must not import the concrete host bridge.
- Domain routers must not mutate React state directly except by calling functions provided on `HostMessageRouterContext`.
- Domain routers must not introduce async work or timers.

## Tests

Keep and extend `hostMessageRouter.test.ts` through the public `routeHostMessage` seam.

Required coverage for this split:

- Existing stale Agent option result behavior.
- Existing stale Task snapshot telemetry without state mutation.
- Existing tool-detail runtime error correlation.
- Existing preference/runtime settings/catalog/custom Agent routing.
- Native session stale result filtering and pagination follow-up.
- Task list archive-mode stale filtering.
- Accepted Task snapshot telemetry, dispatch, navigation task-list refresh, and open-task surface request.
- Runtime error routing for Agent option stale errors and settings errors.

## Out Of Scope

- No host message type renames.
- No App Shell contract changes.
- No reducer action renames.
- No `appController.ts` behavior changes beyond import stability.
- No migration from shell/webview bridge messages to App Server Protocol records in this slice.
