# Frontend Chat Paging Split Implementation

Implemented the accepted Frontend Chat Paging split only.

## Changes

- Kept `state/chatPaging.ts` as the public facade exporting `RenderedChat`,
  `mergePageState`, and `renderedChat`.
- Added `chatPageMerge.ts` for older-page state and message-row
  deduplication.
- Added `chatItemNormalization.ts` for legacy row filtering and legacy Thought
  conversion.
- Added `chatTextCoalescing.ts` for Agent text and Thought run coalescing.
- Added `chatActivityCoalescing.ts` for adjacent activity run coalescing and
  activity title/status derivation.
- Added `chatPaging.test.ts` for facade-level split invariants.

## Preserved Seams

- `TaskView` and reducers continue importing only from `state/chatPaging.ts`.
- No reducer action, component, protocol, or App Shell contracts changed.
- New implementation modules are pure and shell-neutral.

## Preliminary Verification

- `npm run check --workspace openaide-frontend`
- `npm run test --workspace openaide-frontend -- chatPaging.test.ts appReducer.test.ts`
- Boundary import scan for chat paging modules
- Source-size scan for changed production files

## Next Step

Run `$doomsday-review` for the implementation and fix material findings.
