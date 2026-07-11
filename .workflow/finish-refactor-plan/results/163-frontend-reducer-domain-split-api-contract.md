# Frontend Reducer Domain Split: API Contract

## Accepted Contract

Split `appReducer.ts` into focused reducer-domain modules without changing the
public action surface, `AppState` shape, optimistic UI behavior, or App
Shell/App Server contracts.

## Module Layout

Create focused modules under `packages/frontend/src/state/`:

- `appReducer.ts`
  - public `SnapshotIntent` type;
  - public `AppAction` union;
  - public `appReducer(state, action)` entry point;
  - task list, snapshot open/refresh, workspace roots, search/archive, and
    selection actions unless delegated through clear helper functions.
- `newTaskReducer.ts`
  - `reduceNewTaskState(state, action)`;
  - new-task prompt/submission errors;
  - Agent/workspace/isolation selection;
  - config-option loading/result/error;
  - native-session loading/result/error/adoption;
  - new-task attachment add/remove;
  - local `emptyNativeSessions` helper.
- `taskInteractionReducer.ts`
  - `reduceTaskInteractionState(state, action)`;
  - active-task composer prompt/attachments/submit/error;
  - task open errors;
  - chat page loading/result/error;
  - tool-detail loading/result/error;
  - permission responding/error state.
- `settingsReducer.ts`
  - `reduceSettingsState(state, action)`;
  - settings loading/result/error;
  - agent save/delete optimistic acknowledgement state;
  - preferences/runtime/developer patches;
  - selected Settings tab;
  - settings snapshot merge helpers.

No new store package, context, intent API, protocol type, UI component, host
bridge, or generated file is introduced in this slice.

## Reducer API Shape

`appReducer.ts` remains the only module exporting `AppAction` and
`appReducer`.

Domain reducers receive full `AppState` and `AppAction` and return:

- `AppState` when they handle the action;
- `undefined` when the action is outside their domain.

`appReducer` calls domain reducers in an explicit order and returns the first
handled result. It keeps a small switch for global actions that do not belong to
a domain module.

Domain reducers must not throw on unknown actions and must not implement their
own public action unions wider than local helper aliases.

## Ownership Rules

- `AppAction` remains the single public action union used by app code and tests.
- `AppState` remains owned by `store.ts`.
- `appReducer.ts` remains the central dispatch entry point and the only public
  reducer import path.
- Domain reducers may import structured helpers such as `composerOptions`,
  `chatPaging`, and `toolDetailCacheKey`.
- Domain reducers must not import UI components, host bridge services, App
  Server client bindings, or shell/runtime services.
- Domain reducers must preserve the existing responsiveness ladder behavior:
  local state changes remain immediate, pending state remains explicit, and
  rollback/acknowledgement paths remain honest.

## Behavior Invariants

This slice must preserve:

- all action names and payload types;
- `AppState` shape and initial-state compatibility;
- snapshot refresh stale-task guard;
- snapshot open behavior that clears task-open errors, permission responses,
  new-task prompt/submission state, and native-session adoption state;
- workspace root default selection behavior;
- new-task Agent/workspace reset behavior for config options and native
  sessions;
- config-option result replacement behavior;
- native-session page merge behavior and adoption/error cleanup;
- new-task and active-task attachment normalization through `localAttachment`;
- active-task composer pending submit and rollback behavior;
- stale chat page result/error guards;
- tool-detail cache key behavior;
- permission response pending/error behavior;
- search/archive/selection behavior;
- Settings save/delete acknowledgement behavior;
- stale Settings snapshot merge behavior;
- developer runtime settings patch behavior;
- Settings tab selection behavior.

## Out Of Scope

- No action renames.
- No `AppState` shape changes.
- No central intent-layer redesign.
- No new optimistic behavior.
- No UI behavior changes.
- No App Shell or App Server Protocol changes.
- No generated file edits.

## Review Requirements

`$doomsday-review` must check at least:

- `appReducer` remains the only public reducer entry point;
- domain reducers return `undefined` for unhandled actions;
- no domain reducer imports UI, host bridge, App Server client, or shell/runtime
  services;
- behavior covered by `appReducer.test.ts` remains equivalent;
- production reducer source files are under the project source-file size limit.

## Verification Plan

Run:

- `npm run check --workspace openaide-frontend`
- `npm run test --workspace openaide-frontend -- appReducer.test.ts`
- `npm run check`
- `git diff --check`

Also run a source file size check for production Frontend state files and the
updated reducer modules.
