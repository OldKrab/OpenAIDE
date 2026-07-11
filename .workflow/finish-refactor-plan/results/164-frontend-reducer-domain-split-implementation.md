# Frontend Reducer Domain Split: Implementation

## Scope

Implemented the accepted Frontend reducer-domain split without changing the
public reducer surface.

## Changes

- Kept `appReducer.ts` as the public owner of `SnapshotIntent`, `AppAction`,
  and `appReducer`.
- Added `newTaskReducer.ts` for new-task prompt, submit, selection, config
  option, native-session, workspace, and attachment reducer cases.
- Added `taskInteractionReducer.ts` for active task composer, chat page,
  tool-detail, task-open error, and permission response reducer cases.
- Added `settingsReducer.ts` for Settings snapshot, tab, preference,
  developer, runtime, save, delete, and error reducer cases.
- Kept global task list, snapshot, workspace roots, search, archive, and
  selection cases in `appReducer.ts`.
- Kept domain reducers internal to `packages/frontend/src/state`; rendering,
  host bridge, App Server client, shell, and service modules are not imported.

## Preliminary Verification

- `npm run check --workspace openaide-frontend`
- `npm run test --workspace openaide-frontend -- appReducer.test.ts`
- `git diff --check`
- Source-size scan: largest production state file is
  `packages/frontend/src/state/hostMessageRouter.ts` at 322 lines; new reducer
  modules are 196, 177, and 149 lines.
- Boundary scan found no UI, host bridge, App Server client, or service imports
  in `appReducer.ts` or the new reducer modules.

## Next Step

Run `$doomsday-review` for the slice, fix material findings, then run full
integration verification before committing.
