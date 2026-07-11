# Next Slice Selection: Frontend App Controller Split

## Selected Slice

Split `packages/frontend/src/components/App.tsx` into focused Frontend
controller modules while keeping `App.tsx` as the public root component.

## Why This Slice

`App.tsx` is the highest-risk remaining Frontend production file. It is close
to the source-size limit and currently combines several ownership areas:

- bootstrap validation and invalid-surface rendering;
- host message session startup;
- startup requests for navigation, task, and settings surfaces;
- task snapshot polling and telemetry effects;
- config-option and native-session loading effects;
- navigation, settings, task, and new-task surface rendering callbacks.

That makes it the next natural boundary to improve after reducer, Settings, and
Chat message splits. The split should reduce root-component coupling without
changing product behavior or introducing a new App Server protocol shape.

## Candidate Shape

- Keep `App.tsx` as the only exported `App` component and public root.
- Move bootstrap/session wiring into a focused local hook or controller module.
- Move surface-specific render branches into local shell-neutral components or
  render helpers.
- Keep state ownership in `appReducer`, `createInitialState`, and the existing
  state modules.
- Keep host bridge calls behind App-level controller modules; leaf UI
  components should continue receiving typed callbacks and render data.

## Explicit Non-Goals

- Do not change host message types, runtime contracts, reducer actions, visible
  UI text, telemetry names, snapshot request behavior, polling cadence, or
  settings/new-task/task behavior.
- Do not introduce App Server Protocol design changes in this slice.
- Do not split `Composer.tsx` or `AgentSettingsTab.tsx` in this slice unless a
  minimal supporting extraction is required by the App split.

## Risks To Grill

- Startup effects must keep the same ordering and dependency behavior.
- `SnapshotRequestTracker` ownership must stay stable across renders.
- Native-session and config-option request de-duplication keys must not reset
  accidentally.
- Settings preference updates must remain responsive and immediately reflected.
- Surface components must not become new protocol or host-message owners beyond
  the accepted injected callbacks.

## Next Step

Grill and record the API contract for the Frontend App controller split before
implementation.
