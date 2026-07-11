# Next Slice Selection: Frontend App Controller Callbacks Split

## Selected Slice

Split `packages/frontend/src/components/appControllerCallbacks.ts` into focused callback-group modules while keeping `createAppCallbacks` as the public controller intent seam.

## Why This Slice

- `appControllerCallbacks.ts` is the current Frontend user-intent layer for navigation, settings, new-task, and active-task actions.
- It directly posts shell messages and mutates pending presentation state through dispatch, so it is important for the product responsiveness ladder.
- It mixes four callback domains in one large module: navigation, settings, new-task composer/session setup, and active-task commands.
- The public caller surface is already narrow: `appController.ts` calls only `createAppCallbacks`, and rendering surfaces receive grouped callbacks from the returned object.
- Existing tests already prove representative optimistic/pending behavior and host message payloads.

## Scope

In scope:

- Keep `createAppCallbacks(...)` as the public factory used by `appController.ts`.
- Keep `AppControllerCallbacks`, `NavigationCallbacks`, `SettingsCallbacks`, `NewTaskCallbacks`, and `TaskCallbacks` importable from `appControllerCallbacks.ts`.
- Extract focused modules for navigation, settings, new-task, and task callbacks.
- Preserve all dispatch-before-host-message ordering that provides responsive pending UI.
- Preserve all host message types, payload shapes, snapshot metadata, request id usage, and guard behavior.
- Keep shell posting injected or centralized so callback group modules do not grow unrelated app-controller responsibilities.
- Extend `appControllerCallbacks.test.ts` where needed through `createAppCallbacks`.

Out of scope:

- No App Server Protocol redesign.
- No host message type or payload rename.
- No App Shell/VS Code changes.
- No reducer action rename.
- No behavior change to new-task submission, native-session adoption, task prompts, settings updates, archive toggles, tool detail loading, or permission responses.

## Review Risks

- Moving callbacks can accidentally change dispatch/post ordering and make UI feel less responsive.
- Snapshot request id creation can move to the wrong callback or be skipped.
- Guard paths such as `state.newTask.submitting`, missing `state.snapshot`, and cached tool details can regress.
- Domain modules can become shallow pass-through wrappers if their interface is not kept behind `createAppCallbacks`.

## Proposed Next Packet

Grill and accept the API contract for the Frontend App Controller Callbacks split before implementation.
