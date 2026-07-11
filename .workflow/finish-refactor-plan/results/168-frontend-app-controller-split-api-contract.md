# Frontend App Controller Split: API Contract

## Goal

Split `packages/frontend/src/components/App.tsx` into focused local controller
and surface modules without changing the public root component or product
behavior.

## Public Surface

- `App.tsx` remains the only module exporting `App`.
- Existing re-exports from `App.tsx` are preserved:
  - `firstToolPath`;
  - `newTaskStatusLabel`;
  - `relativeTime`;
  - `taskWorkingStatusLabel`.
- No new public package entry point is introduced.

## Module Ownership

### `App.tsx`

- Owns only the public root component wrapper.
- Calls the accepted controller hook and surface renderer.
- Does not contain host-message effect bodies, request-key refs, polling
  intervals, or large surface render branches after the split.

### `appController.ts`

- Exports `useAppController()`.
- Owns bootstrap loading, reducer initialization, local preference and agent
  state, `SnapshotRequestTracker`, request-id refs, native-session/config
  option request de-duplication refs, host message session startup, startup
  requests, task snapshot fallback, active-task polling, task-render telemetry,
  config-option loading, and native-session loading.
- Imports host bridge and host-message session APIs:
  `getBootstrap`, `postHostMessage`, `subscribeHostMessages`,
  `startHostMessageSession`, `routeHostMessage`, telemetry helpers, and
  surface coordinator helpers.
- Returns render-ready state and typed callback groups for surfaces.

### `AppSurfaces.tsx`

- Exports `AppSurfaces`.
- Owns the root surface switch for invalid, navigation, settings, task-loading,
  active-task, and new-task surfaces.
- Receives controller data and callback groups as props.
- May import shell-neutral UI components only.
- Must not import host bridge functions, host-message session functions,
  `routeHostMessage`, or App Server client/runtime services.

## Callback Contract

`useAppController()` returns a structured object with:

- `bootstrap`;
- `state`;
- `dispatch`;
- `preferences`;
- `agents`;
- `activeTask`;
- `visibleTasks`;
- `createSnapshotRequestId`;
- navigation callbacks:
  - `openTask`;
  - `archiveTask`;
  - `restoreTask`;
  - `openNativeSession`;
  - `changeSearch`;
  - `toggleArchived`;
  - `loadNativeSessions`;
  - `openNewTask`;
  - `openSettings`;
- settings callbacks:
  - `refreshSettings`;
  - `authenticateAgent`;
  - `deleteCustomAgent`;
  - `saveCustomAgent`;
  - `setAgentEnabled`;
  - `unlockDeveloperSettings`;
  - `setAcpTrace`;
  - `setComposerSubmitShortcut`;
  - `selectSettingsTab`;
- new-task callbacks:
  - `submitNewTask`;
  - `pickNewTaskFileContext`;
  - `selectConfigOption`;
  - `resetOptionsRequestKey`.
- task callbacks:
  - `sendTaskPrompt`;
  - `loadChatPage`;
  - `loadToolDetail`;
  - `respondToPermission`;
  - `cancelTask`;
  - `pickTaskFileContext`.

Exact TypeScript names may be adjusted during implementation if the ownership
and behavior contract remains unchanged.

## Behavioral Invariants

- Preserve all host message types and payload shapes.
- Preserve telemetry event names and payload fields.
- Preserve `SnapshotRequestTracker` semantics, snapshot request ids, snapshot
  intents, navigation-change handling, and task open fallback behavior.
- Preserve task active polling cadence at 600 ms and task open fallback delay at
  1200 ms.
- Preserve config-option and native-session request de-duplication behavior.
- Preserve reducer action names and dispatch ordering.
- Preserve Settings preference responsiveness: composer shortcut changes update
  local preference state and Settings state immediately before posting the host
  message.
- Preserve invalid-surface, navigation, Settings, Task, loading, and new-task UI
  text, CSS class names, ARIA labels, and callback behavior.
- Keep responsive UI behavior unchanged: local presentation updates remain
  immediate, pending states still dispatch before host messages, and long local
  work remains visible through existing state.

## Boundary Rules

- `AppSurfaces.tsx`, `NewTaskView.tsx`, and `TaskView.tsx` must not import
  host bridge, host-message session, App Server client, runtime service, or
  protocol modules. The previously accepted `ChatToolBlocks.tsx` tool-path
  host bridge exception remains unchanged.
- `appController.ts` may import state, host bridge, host-message routing, and
  surface coordinator modules because it is the local Frontend controller for
  the current shell bridge.
- The split must not move product workflow decisions into leaf rendering
  components.
- The split must not introduce protocol changes, storage changes, reducer state
  shape changes, or new shell-specific product APIs.

## Tests And Verification

Implementation must pass:

- `npm run check --workspace openaide-frontend`;
- focused Frontend tests relevant to `App`, host-message routing, and state;
- `npm run check`;
- `git diff --check`;
- source-size scan for production files.

Review must use `$doomsday-review` with subagents for correctness,
requirements/tests, and code quality.

## Next Step

Implement only this split, record the implementation artifact, run
`$doomsday-review`, fix material findings, run integration verification, and
commit the slice.
