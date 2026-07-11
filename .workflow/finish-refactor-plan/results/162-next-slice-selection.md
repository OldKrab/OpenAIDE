# Next Slice Selection: Frontend Reducer Domain Split

## Decision

Select the Frontend reducer-domain split as the next refactor slice.

## Why This Slice

`packages/frontend/src/state/appReducer.ts` is 537 lines and is now the largest
remaining production Frontend source file. It mixes several independent state
domains:

- task list, selection, snapshot opening, and archive/search filters;
- new-task composer selection, workspace roots, config options, native sessions,
  and attachments;
- active-task follow-up composer state;
- chat page loading and tool-detail cache state;
- permission response state;
- Settings loading, snapshots, preferences, runtime settings, and Agent
  save/delete optimistic acknowledgement state.

The architecture requires a central intent/state layer for responsive UI.
This slice keeps the single `appReducer` entry point while moving domain
reducers into focused state modules so future intent-layer work can attach to
clear state ownership boundaries.

## Slice Boundary

Split `appReducer.ts` into local reducer-domain modules under
`packages/frontend/src/state/`:

- keep `appReducer.ts` as the public `AppAction` type owner and central
  dispatch entry point;
- move new-task actions and native-session helpers into a focused new-task
  reducer module;
- move task input, chat page, tool-detail, and permission response actions into
  a focused task interaction reducer module;
- move Settings actions and Settings snapshot merge helpers into a focused
  Settings reducer module;
- keep task list, snapshot, search/archive, and selection routing in
  `appReducer.ts` unless the API contract chooses a cleaner small helper;
- preserve all action names, payload types, state shapes, optimistic/pending
  behavior, and test behavior.

## Out Of Scope

- No reducer action renames.
- No `AppState` shape changes.
- No central intent-layer redesign in this slice.
- No App Server Protocol, App Shell, or host bridge changes.
- No UI behavior changes.
- No generated file edits.

## Risk Map For API Grill

- `AppAction` must remain the single public action union consumed by the
  existing app code and tests.
- Domain reducers must return `undefined` or a typed result for unhandled
  actions so `appReducer` remains the only exhaustive action switch owner.
- New-task reducer must preserve config-option replacement, workspace/agent
  reset behavior, local attachment normalization, and native-session merge and
  adoption state.
- Task interaction reducer must preserve pending composer rollback,
  stale-chat-page guards, tool-detail cache keys, and permission response state.
- Settings reducer must preserve optimistic Agent save/delete behavior,
  stale snapshot merge rules, developer settings patching, and tab selection.
- This is a mechanical boundary split, not a behavior change.
