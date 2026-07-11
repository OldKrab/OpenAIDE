# Next Slice Selection: Frontend Settings Split

## Decision

Select the Frontend Settings split as the next refactor slice.

## Why This Slice

`packages/frontend/src/components/settings/SettingsView.tsx` is 849 lines and
mixes:

- the Settings page shell and tab keyboard behavior;
- Agent settings master/detail state;
- custom Agent draft and acknowledgement helpers;
- Agent status and environment editing;
- MCP, Skills, and General tab rendering;
- shared settings presentation helpers.

This is the largest production Frontend source file and sits directly in the
plan's next major area: move shared UI into shell-neutral Frontend with clear
injection points and no product workflow decisions in rendering components.

## Slice Boundary

Split `SettingsView.tsx` into local Settings components under
`packages/frontend/src/components/settings/`:

- keep `SettingsView.tsx` as the page shell, tab selection, loading/error
  surface, and tab routing owner;
- move Agent-specific UI and draft acknowledgement helpers to an Agent tab
  module;
- move MCP, Skills, and General tab renderers into tab-specific modules;
- move reusable settings presentation helpers into a local shared module;
- preserve all public component props, settings state shape, callbacks, CSS
  class names, ARIA attributes, and visible text.

## Out Of Scope

- No Settings UX redesign.
- No App Server Protocol changes.
- No app-shell-contracts type changes.
- No settings state reducer changes.
- No CSS class renames or styling changes unless needed for imports.
- No localization or copy changes.

## Risk Map For API Grill

- `SettingsView` must not become a pass-through wrapper with hidden state in
  unrelated modules.
- Agent tab owns local draft UI state, but product settings state remains in
  `SettingsState` and Backend snapshots.
- Acknowledgement helper exports must remain testable after moving files.
- Tab modules should receive typed props and callbacks, not import host bridge,
  reducer actions, or App Server client logic directly.
- Shared helper module must stay presentation-only and not become a generic
  settings framework.
