# Frontend Composer Split: API Contract

## Goal

Split `packages/frontend/src/components/Composer.tsx` into focused
shell-neutral modules without changing the public Composer API, keyboard
behavior, or Task/New Task composer behavior.

## Public Surface

- `Composer.tsx` remains the public component imported by `TaskView.tsx` and
  `NewTaskView.tsx`.
- `Composer` keeps the same props:
  - `agentLocked`;
  - `attachments`;
  - `configLocked`;
  - `configOptions`;
  - `disabled`;
  - `agents`;
  - `onAddWorkspaceContext`;
  - `onCancel`;
  - `onChange`;
  - `onPickFileContext`;
  - `onRemoveAttachment`;
  - `onSelectAgent`;
  - `onSelectConfigOption`;
  - `onSelectIsolation`;
  - `onSubmit`;
  - `placeholder`;
  - `prompt`;
  - `selection`;
  - `submitShortcut`;
  - `submitDisabled`.
- `shouldSubmitComposerKey` remains importable from `Composer.tsx` for
  existing tests and consumers.
- No TaskView, NewTaskView, reducer, host bridge, App Server protocol, or App
  Shell contract changes are part of this slice.

## Module Ownership

### `Composer.tsx`

- Owns the public `Composer` component.
- Owns `openMenu` state and Escape-to-close behavior.
- Owns top-level submit shortcut handling in the textarea.
- Composes extracted modules and forwards typed callbacks.

### `composerKeymap.ts`

- Owns `shouldSubmitComposerKey`.
- Contains no React rendering, host bridge imports, reducer imports, or side
  effects.
- `Composer.tsx` re-exports `shouldSubmitComposerKey`.

### `ComposerAttachments.tsx`

- Owns attachment token rendering only.
- Receives attachments, disabled state, and remove callback.
- Preserves `composer-attachments`, `context-token`, visible labels, icons,
  remove buttons, and remove ARIA labels.

### `ComposerPrimitives.tsx`

- Owns reusable local rendering primitives:
  - icon button;
  - selector pill;
  - popover;
  - menu button.
- Preserves class names, roles, `aria-expanded`, `aria-pressed`,
  `aria-checked`, disabled behavior, locked rendering, and visible text.
- Remains local to Composer modules; not a global design-system abstraction.

### `ComposerMenus.tsx`

- Owns Agent, config-option, isolation, and add-context menu rendering.
- Receives current `openMenu`, selection/config/agent data, and callbacks from
  `Composer.tsx`.
- Owns config-option label/icon helpers and menu id helper unless a smaller
  pure helper module is clearer.
- Preserves current menu labels, descriptions, icon choices, active radio
  state, disabled add-workspace behavior, callback behavior, and menu class
  names.

## Behavioral Invariants

- Escape closes the currently open menu.
- Textarea `onChange` and value behavior remain unchanged.
- Submit shortcut behavior remains unchanged and covered by
  `Composer.test.ts`.
- `submitDisabled` still blocks keyboard submit and send button submit.
- Add-context menu may still open while the composer is disabled only through
  the existing `disabled && menu !== "add"` exception.
- Agent and isolation selectors still lock when `agentLocked` is true.
- Config selectors still lock when `configLocked` is true or composer is
  disabled.
- Add workspace remains disabled when `selection.workspaceRoot` is empty and
  keeps the same `"No workspace open."` description.
- Agent menu still filters disabled Agents.
- Config menus still use `option.current_value`, option/value descriptions,
  and existing icon category mapping.
- Isolation menu still uses `isolationOptions` labels and descriptions.
- Cancel button still replaces send button when `onCancel` is present.
- Visible text, class names, roles, ARIA labels, titles, icons, and layout
  structure remain unchanged.

## Boundary Rules

- Composer modules must not import host bridge functions, App Server clients,
  reducer actions, protocol modules, shell/runtime services, App controller
  modules, or Settings modules.
- Composer modules remain shell-neutral UI and callback presenters.
- No product workflow decisions move into Composer modules.
- No broad visual redesign, attachment redesign, menu behavior redesign, or
  settings/config option behavior changes are part of this slice.

## Tests And Verification

Implementation must pass:

- `npm run check --workspace openaide-frontend`;
- `npm run test --workspace openaide-frontend -- Composer.test.ts`;
- any new focused tests needed for moved pure helpers or menu behavior;
- `npm run check`;
- `git diff --check`;
- production source-size scan.

Review must use `$doomsday-review` with subagents for correctness,
requirements/tests, and code quality.

## Next Step

Implement only this split, record the implementation artifact, run
`$doomsday-review`, fix material findings, run integration verification, and
commit the slice.
