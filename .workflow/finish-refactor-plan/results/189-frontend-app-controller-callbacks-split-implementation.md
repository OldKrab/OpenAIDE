# Frontend App Controller Callbacks Split Implementation

## Scope

Implemented the accepted Frontend App Controller Callbacks split only.

## Changes

- Kept `createAppCallbacks(...)` as the public user-intent seam used by `appController.ts`.
- Kept `AppControllerCallbacks`, `NavigationCallbacks`, `SettingsCallbacks`, `NewTaskCallbacks`, and `TaskCallbacks` importable from `appControllerCallbacks.ts`.
- Added `appControllerCallbackTypes.ts` for public callback group types and factory dependency types.
- Added focused callback group modules:
  - `navigationCallbacks.ts`
  - `settingsCallbacks.ts`
  - `newTaskCallbacks.ts`
  - `taskCallbacks.ts`
- Preserved dispatch-before-host-message ordering, request id generation, guard paths, host message payloads, and existing `AppSurfaces` wiring.
- Extended `appControllerCallbacks.test.ts` through `createAppCallbacks` for archive behavior, archive-mode toggle, config-option mutation, tool-detail cache guards, and missing-snapshot no-ops.

## Preliminary Verification

- `npm run check --workspace openaide-frontend`
- `npm run test --workspace openaide-frontend -- appControllerCallbacks.test.ts`
- `npm run check`
- `npm test -- --runInBand`
- `git diff --check`
- App Controller Callbacks source-size scan
- App Controller Callbacks boundary scan for rendering, router, App Server client, and settings UI imports
