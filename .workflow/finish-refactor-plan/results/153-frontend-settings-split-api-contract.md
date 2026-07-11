# Frontend Settings Split: API Contract

## Accepted Contract

Split the oversized Settings page into local shell-neutral Frontend modules
without changing Settings behavior, styling hooks, product state ownership, or
App Shell/App Server contracts.

## Module Layout

Create focused modules under `packages/frontend/src/components/settings/`:

- `SettingsView.tsx`
  - public page component;
  - tab list and keyboard navigation;
  - developer-settings unlock click counter;
  - loading/error/snapshot surface;
  - tab routing to the focused tab components.
- `AgentSettingsTab.tsx`
  - Agent tab master/detail rendering;
  - custom Agent draft state;
  - custom Agent save/delete pending acknowledgement consumption;
  - Agent status panel;
  - custom Agent icon picker;
  - custom Agent environment editor;
  - exports `shouldConsumeAgentSaveAck` and
    `shouldConsumeAgentDeleteAck` for existing unit coverage.
- `McpSettingsTab.tsx`
  - MCP settings list and empty state rendering.
- `SkillsSettingsTab.tsx`
  - Skill settings list and empty state rendering.
- `GeneralSettingsTab.tsx`
  - General settings search state;
  - composer preference row rendering;
  - developer diagnostics row rendering.
- `settingsPresentation.tsx`
  - reusable presentation-only helpers:
    `StatusBadge`, `InlineFailure`, `InlineNotice`, `EmptySettingsState`,
    `SettingsSkeleton`, and status label formatting if needed.

No new package, protocol type, reducer, host bridge, or state-management module
is introduced in this slice.

## Component API

`SettingsView` keeps its current public props exactly:

- `onAuthenticate(agentId, methodId)`;
- `onDeleteCustomAgent(agentId)`;
- `onSaveCustomAgent(params)`;
- `onSetAgentEnabled(agentId, enabled)`;
- `onUnlockDeveloperSettings()`;
- `onRefresh()`;
- `onSetAcpTrace(enabled)`;
- `onSetComposerSubmitShortcut(shortcut)`;
- `onSelectTab(tab)`;
- `state`.

Tab components receive only the render data and callbacks they currently use:

- `AgentSettingsTab`
  - `agents`;
  - `authPending`;
  - `deletedAgentId`;
  - `savedAgentId`;
  - `onAuthenticate`;
  - `onDeleteCustomAgent`;
  - `onSaveCustomAgent`;
  - `onSetAgentEnabled`.
- `McpSettingsTab`
  - `servers`;
  - `workspaceBlock`.
- `SkillsSettingsTab`
  - `skills`.
- `GeneralSettingsTab`
  - `common`;
  - `onSetAcpTrace`;
  - `onSetComposerSubmitShortcut`.

Tab components must not import `SettingsState`, App Server client bindings,
host bridge functions, reducer actions, or shell/runtime services.

## Ownership Rules

- Backend/App Server snapshots remain the source of truth for Settings product
  state.
- `SettingsView` owns only page-level UI state:
  - active tab is still provided by `SettingsState.activeTab`;
  - developer unlock click counter remains local;
  - tab focus behavior remains local.
- `AgentSettingsTab` may own local presentation state for selected Agent,
  custom Agent draft, delete confirmation, and pending save/delete
  acknowledgements.
- `GeneralSettingsTab` may own local search text.
- Shared presentation helpers may only format/render existing values. They must
  not make product workflow decisions, call callbacks, or start host/backend
  work.

## Behavior Invariants

This slice must preserve:

- all Settings visible text;
- all CSS class names;
- tab ids, panel ids, ARIA roles, selected state, keyboard navigation, and focus
  behavior;
- refresh button disabled/loading behavior;
- Settings error and skeleton behavior;
- developer unlock click threshold and callback behavior;
- Agent list selection, draft edit/create/save/delete behavior;
- Agent authentication button enabled/disabled rules;
- built-in Agent enabled toggle behavior;
- custom Agent environment editing behavior;
- MCP, Skills, and General empty/error/list rendering;
- General settings search behavior;
- exported acknowledgement helper behavior covered by
  `SettingsView.test.tsx`.

## Out Of Scope

- No visual redesign.
- No copy changes.
- No reducer/action changes.
- No Settings snapshot shape changes.
- No App Shell or App Server Protocol changes.
- No CSS class renames.
- No generated file edits.

## Review Requirements

`$doomsday-review` must check at least:

- `SettingsView` still exposes the same public props and behavior;
- tab modules do not reach across the Frontend/App Shell/App Server boundary;
- extracted presentation helpers stay presentation-only;
- Agent custom draft acknowledgement behavior remains covered and equivalent;
- keyboard/ARIA ids and visible text are not unintentionally changed;
- production source files are under the project source-file size limit.

## Verification Plan

Run:

- `npm run check --workspace openaide-frontend`
- `npm run check`
- `git diff --check`

Also run a source file size check for production Frontend files and the updated
Settings modules.
