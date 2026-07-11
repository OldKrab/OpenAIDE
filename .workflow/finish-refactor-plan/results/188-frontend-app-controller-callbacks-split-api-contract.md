# Frontend App Controller Callbacks Split API Contract

## Decision

Accept the Frontend App Controller Callbacks split.

The external seam remains:

```ts
createAppCallbacks(dependencies): AppControllerCallbacks
```

`appController.ts` remains the only production caller. Rendering modules continue receiving callback groups from `AppControllerCallbacks`; they must not import the new callback-group implementation modules.

## Public Exports

`appControllerCallbacks.ts` keeps exporting:

- `AppControllerCallbacks`
- `NavigationCallbacks`
- `SettingsCallbacks`
- `NewTaskCallbacks`
- `TaskCallbacks`
- `createAppCallbacks`

No new production-facing callback factory exports are added. Internal callback group builders may be exported only if tests require it, but preferred tests stay on `createAppCallbacks`.

## Internal Modules

Create focused private modules under `packages/frontend/src/components/`:

- `navigationCallbacks.ts`
  - `archiveTask`
  - `changeSearch`
  - `loadNativeSessions`
  - `openNativeSession`
  - `openNewTask`
  - `openSettings`
  - `openTask`
  - `restoreTask`
  - `toggleArchived`

- `settingsCallbacks.ts`
  - Agent authentication, custom Agent save/delete, settings refresh, tab selection, ACP trace toggle, Agent enable toggle, composer shortcut update, developer unlock.

- `newTaskCallbacks.ts`
  - File context picking for new-task composer.
  - Options request key reset.
  - Config option mutation.
  - New Task submit.

- `taskCallbacks.ts`
  - Task cancel.
  - Chat page loading.
  - Tool detail loading and cache guard.
  - File context picking for an active Task.
  - Permission response.
  - Follow-up prompt send.

- `appControllerCallbackTypes.ts`
  - Shared callback group types and dependency input types if needed to keep group modules small and avoid circular imports.

If a type-only module is unnecessary after implementation, keep types in `appControllerCallbacks.ts`. Avoid creating an extra public-looking seam without need.

## Behavior To Preserve

- `archiveTask` clears selection before posting archive if the archived Task is active.
- `archiveTask` and `restoreTask` post current `state.showArchived` in the `archived` payload.
- `toggleArchived` calls `beginNavigationChange(showArchived)`, dispatches `archive:set`, then posts `task.list` for the new mode.
- `openNativeSession` no-ops while `state.newTask.submitting`; otherwise it dispatches adoption before posting `task.create` with `adopt_external_session`, `snapshot_intent: "open"`, and a fresh snapshot request id.
- `openTask` dispatches selection before posting `surface.openTask` with the known title when present.
- Settings callbacks that start host work dispatch `settings:start` before posting.
- `setComposerSubmitShortcut` updates local preferences and reducer state before posting the host preference request.
- `setAcpTrace` dispatches the local developer trace state before posting the host request.
- `selectSettingsTab` remains local-only.
- New-task `selectConfigOption` computes the Agent/workspace options key, stores it in `latestOptionsRequestKey.current`, dispatches `newTask:configOptions:start`, then posts `session.setConfigOption`.
- New-task `submit` dispatches `submit:start` before posting `task.create` with prompt text, selected Agent/isolation/config options, protocol attachments, `snapshot_intent: "open"`, and a fresh snapshot request id.
- Task callbacks no-op when `state.snapshot` is missing.
- `loadToolDetail` no-ops when the selected tool detail is already loading or already loaded.
- `respondToPermission` dispatches `permission:responding` before posting `permission.respond` with refresh snapshot metadata.
- `sendPrompt` posts `session.prompt` with protocol attachments and refresh snapshot metadata, then dispatches `taskInput:submit`.

## Boundary Rules

- The public App Controller intent seam remains `createAppCallbacks`.
- Callback-group modules may depend on reducer action types, App state, composer attachment helpers, surface coordinator helpers, surface routing helpers, and App Shell message types.
- Callback-group modules must not import rendering components, host message routers, App Server client bindings, settings UI modules, or reducers beyond action/state types.
- Concrete shell posting stays centralized through the same `postHostMessage` helper unless implementation chooses to inject a `postHostMessage` dependency for testability without changing the public seam.
- No async work, timers, or direct DOM/browser access in callback group modules.

## Tests

Keep and extend `appControllerCallbacks.test.ts` through `createAppCallbacks`.

Required coverage for this split:

- Existing new-task submit pending-before-post behavior.
- Existing local composer shortcut preference update before host request.
- Existing task prompt send payload and pending input dispatch.
- Existing permission response pending-before-post behavior.
- Existing native session adoption pending-before-post behavior.
- Archive active Task clears selection before host archive request.
- Toggle archive begins navigation change, updates local archive state, and posts the new list mode.
- New-task config option mutation updates the latest options key, starts loading state, and posts `session.setConfigOption`.
- Tool detail loading no-ops when details are already loading or loaded.
- Task callbacks no-op without a snapshot.

## Out Of Scope

- No changes to `AppSurfaces.tsx` props.
- No changes to `useAppController` effects.
- No changes to host bridge implementation.
- No reducer behavior changes beyond preserving current actions.
