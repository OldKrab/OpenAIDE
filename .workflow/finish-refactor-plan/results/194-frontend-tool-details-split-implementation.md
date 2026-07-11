# Frontend Tool Details Split Implementation

Implemented the accepted Frontend Tool Details split only.

## Changes

- Kept `ChatToolDetailsView.tsx` as the public `ChatToolDetails` router.
- Extracted concrete tool-detail renderers:
  - `ReadToolDetails.tsx`
  - `EditToolDetails.tsx`
  - `SearchToolDetails.tsx`
  - `ExecuteToolDetails.tsx`
  - `GenericToolDetails.tsx`
- Kept `state/toolDetailsViewModel.ts` as a compatibility export module.
- Extracted pure view-model modules:
  - `toolDetailsShared.ts`
  - `toolCommandViewModel.ts`
  - `toolEditViewModel.ts`
  - `toolSearchViewModel.ts`
  - `toolExecuteViewModel.ts`
- Added `toolDetailsViewModel.test.ts` for helper-level parsing, fallback, and
  path-priority behavior.

## Preserved Seams

- `ChatToolDetails` remains the only production tool-details renderer entry.
- `ChatToolBlocks.tsx` public exports and `ToolPath` host message behavior are
  unchanged.
- Existing compatibility imports from `state/toolDetailsViewModel.ts` remain
  valid.
- `firstToolPath` remains available through existing public re-export paths.

## Preliminary Verification

- `npm run check --workspace openaide-frontend`
- `npm run test --workspace openaide-frontend -- ChatMessageView.test.tsx App.test.tsx toolDetailsViewModel.test.ts`
- Source-size scan for changed production files

## Next Step

Run `$doomsday-review` for the implementation and fix material findings.
