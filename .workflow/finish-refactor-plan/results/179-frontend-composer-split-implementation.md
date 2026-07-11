# Frontend Composer Split Implementation

## Scope

Implemented the accepted Frontend Composer split only.

## Changes

- Kept `Composer.tsx` as the public component with unchanged props.
- Kept `shouldSubmitComposerKey` importable from `Composer.tsx` and moved the implementation to `composerKeymap.ts`.
- Added `ComposerAttachments.tsx` for attachment token rendering.
- Added `ComposerPrimitives.tsx` for local composer icon button, selector, popover, and menu button primitives.
- Added `ComposerMenus.tsx` for add-context, Agent, config-option, and isolation menu rendering.
- Preserved Task/New Task call sites, shell boundaries, visible text, class names, roles, ARIA labels, icons, locked/disabled behavior, Escape close behavior, and submit shortcut behavior.

## Preliminary Verification

- `npm run check --workspace openaide-frontend`
- `npm run test --workspace openaide-frontend -- Composer.test.ts`
- `npm run check`
- `npm test -- --runInBand`
- `git diff --check`
- Composer source-size scan
- Composer boundary scan for host bridge, App Server client, reducer, service, and settings imports
