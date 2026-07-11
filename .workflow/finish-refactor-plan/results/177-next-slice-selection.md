# Next Slice Selection: Frontend Composer Split

## Selected Slice

Split `packages/frontend/src/components/Composer.tsx` into focused
shell-neutral composer modules while keeping `Composer.tsx` as the public
Composer component and `shouldSubmitComposerKey` helper owner.

## Why This Slice

`Composer.tsx` is now the largest Frontend production component at 371 lines.
It combines several responsibilities:

- text input and submit shortcut handling;
- attachment token rendering;
- add-context menu rendering;
- Agent, config-option, and isolation selector controls;
- generic icon button, popover, and menu button primitives;
- config-option label/icon helpers;
- submit shortcut helper.

The split keeps moving Frontend toward smaller shell-neutral modules while
preserving responsive composer behavior and the current Task/New Task call
sites.

## Candidate Shape

- Keep `Composer.tsx` as the public component used by `TaskView` and
  `NewTaskView`.
- Keep `shouldSubmitComposerKey` importable from `Composer.tsx`.
- Extract focused local modules:
  - attachment token list;
  - generic composer primitives such as icon button, selector, popover, and
    menu button;
  - Agent/config/isolation menu rendering;
  - config-option label/icon helpers.
- Keep `openMenu` state in `Composer.tsx` unless implementation proves a
  smaller state owner is cleaner without changing behavior.

## Explicit Non-Goals

- Do not change Task or New Task submit behavior.
- Do not change visible text, class names, roles, ARIA labels, icons, locked
  labels, disabled behavior, menu ids, or keyboard behavior.
- Do not add host bridge, App Server client, reducer, protocol, shell/runtime,
  or App controller imports to Composer modules.
- Do not redesign attachments, file selection, config options, or Agent
  selection in this slice.

## Risks To Grill

- The Escape handler must still close the open menu.
- Disabled composer controls must preserve the existing add-menu exception and
  selector lock behavior.
- Config-option menus must keep the current active value, icon, description,
  and callback behavior.
- Submit shortcut behavior must stay covered by `Composer.test.ts`.

## Next Step

Grill and record the API contract for the Frontend Composer split before
implementation.
