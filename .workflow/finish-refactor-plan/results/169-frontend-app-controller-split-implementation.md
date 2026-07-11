# Frontend App Controller Split: Implementation

## Scope

Implemented the accepted Frontend App controller split without changing the
public root component or host/runtime behavior.

## Changes

- Kept `App.tsx` as the public `App` root and preserved existing re-exports.
- Added `appController.ts` with `useAppController()` for bootstrap, reducer
  setup, host message session startup, startup requests, snapshot request refs,
  config-option and native-session request de-duplication refs, polling, and
  telemetry effects.
- Added `appControllerCallbacks.ts` for host-aware surface callback groups.
- Added `AppSurfaces.tsx` for invalid, navigation, settings, task-loading,
  active-task, and new-task surface rendering.
- Removed direct host bridge/protocol command imports from `NewTaskView.tsx`
  and `TaskView.tsx`; they now receive typed callbacks.
- Kept the previously accepted `ChatToolBlocks.tsx` tool-path host bridge
  exception unchanged.

## Preliminary Verification

- `npm run check --workspace openaide-frontend`
- `npm run test --workspace openaide-frontend -- App.test.tsx hostMessageRouter.test.ts appReducer.test.ts`
- Source-size scan: largest changed production files are
  `appControllerCallbacks.ts` at 337 lines, `appController.ts` at 256 lines,
  `TaskView.tsx` at 180 lines, `NewTaskView.tsx` at 108 lines,
  `AppSurfaces.tsx` at 99 lines, and `App.tsx` at 10 lines.
- Boundary scan found no host bridge, host-message session, App Server client,
  protocol attachment, or tool-detail request imports in `AppSurfaces.tsx`,
  `NewTaskView.tsx`, or `TaskView.tsx`.

## Next Step

Run `$doomsday-review` for the slice, fix material findings, then run
integration verification before committing.
