# Frontend Agent Settings Tab Split: API Contract

## Goal

Split `packages/frontend/src/components/settings/AgentSettingsTab.tsx` into
focused Settings modules without changing the public tab component, Settings
behavior, or callback contracts.

## Public Surface

- `AgentSettingsTab.tsx` remains the public component imported by
  `SettingsView.tsx`.
- `AgentSettingsTab` keeps the same props:
  - `agents`;
  - `authPending`;
  - `onAuthenticate`;
  - `onDeleteCustomAgent`;
  - `onSaveCustomAgent`;
  - `onSetAgentEnabled`;
  - `deletedAgentId`;
  - `savedAgentId`.
- `shouldConsumeAgentSaveAck` and `shouldConsumeAgentDeleteAck` remain
  importable from `AgentSettingsTab.tsx` for the existing Settings tests unless
  a helper module re-export preserves that import path.
- No package entry points, host message contracts, protocol contracts, reducer
  actions, or SettingsView props change.

## Module Ownership

### `AgentSettingsTab.tsx`

- Owns tab-level state:
  - selected Agent id;
  - active custom Agent draft;
  - delete confirmation id;
  - pending save id;
  - pending delete id.
- Owns acknowledgement effects that consume `savedAgentId` and
  `deletedAgentId`.
- Owns orchestration callbacks that update draft/selection/pending state and
  call the parent callback props.
- Renders the high-level Settings panel layout by composing extracted local
  components.

### Agent Settings Helper Module

- Owns pure draft helpers and acknowledgement predicates:
  - `AgentDraft`;
  - `draftFromAgent`;
  - `newAgentDraft`;
  - `shouldConsumeAgentSaveAck`;
  - `shouldConsumeAgentDeleteAck`;
  - `agentStatusCopy` if shared by extracted components.
- Has no React state, host bridge imports, reducer imports, App Server client
  imports, shell imports, or side effects.

### Agent List Component

- Receives `agents`, selected id, draft-active state, and `onSelectAgent`.
- Renders only the master Agent list and Add button callback.
- Must preserve list role, labels, class names, selected styling, status badge,
  icon rendering, and visible text.

### Agent Detail Component Group

- Receives render data, callbacks, and current draft facts from
  `AgentSettingsTab`.
- Owns presentational rendering for:
  - detail header and primary authentication button;
  - built-in Agent status panel;
  - built-in launch/read-only rows;
  - built-in availability toggle;
  - custom Agent draft launch fields;
  - custom Agent action buttons.
- Must not own save/delete acknowledgement effects or parent callback
  invocation policy beyond explicit button callbacks passed from the tab.

### Custom Agent Leaf Components

- `AgentIconPicker` owns only icon radio rendering.
- `AgentEnvEditor` owns only environment-row rendering and row patching.
- They receive values and callbacks only.
- They must preserve visible text, placeholders, labels, class names, roles,
  and ARIA attributes.

## Behavioral Invariants

- Selecting an Agent clears draft state and delete confirmation state.
- Adding a custom Agent creates a new draft with the same defaults as today.
- Editing custom Agent fields updates only local draft state until Save.
- Saving clears delete confirmation, records the pending save id, and calls
  `onSaveCustomAgent` with the same payload shape and values.
- A created custom Agent acknowledgement still closes the draft and selects the
  saved Agent id.
- Editing an existing custom Agent acknowledgement still closes the draft and
  selects that Agent only when the saved id matches the pending save id.
- Deleting a custom Agent still requires the same second click confirmation.
- A delete acknowledgement still clears the draft, clears pending delete state,
  and selects the first remaining Agent using the current fallback behavior.
- Built-in Agent enabled toggles still call `onSetAgentEnabled` immediately.
- Custom Agent enabled toggles still update draft state only.
- Authentication buttons keep the same primary auth-method selection, disabled
  behavior, and callback args.
- Existing visible text, class names, roles, ARIA labels, titles, icons, and
  Settings tab layout remain unchanged.

## Boundary Rules

- Extracted Agent Settings modules must not import host bridge functions, App
  Server clients, reducer actions, protocol modules, shell/runtime services, or
  App controller modules.
- Extracted components remain shell-neutral Settings UI.
- No new product decisions move into leaf components; `AgentSettingsTab.tsx`
  remains the local state and acknowledgement owner.
- No broad visual redesign, validation redesign, persistence changes, or
  Settings data shape changes are part of this slice.

## Tests And Verification

Implementation must pass:

- `npm run check --workspace openaide-frontend`;
- focused Settings tests for save/delete acknowledgement helpers;
- any new focused tests needed for moved pure helpers or callbacks;
- `npm run check`;
- `git diff --check`;
- production source-size scan.

Review must use `$doomsday-review` with subagents for correctness,
requirements/tests, and code quality.

## Next Step

Implement only this split, record the implementation artifact, run
`$doomsday-review`, fix material findings, run integration verification, and
commit the slice.
