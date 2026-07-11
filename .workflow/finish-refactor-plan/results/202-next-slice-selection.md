# Next Slice Selection: Frontend Chat Paging Split

## Decision

Select the Frontend Chat Paging split as the next refactor slice.

## Why This Slice

`packages/frontend/src/state/chatPaging.ts` is now the largest Frontend state
module. It is pure and shell-neutral, but it currently mixes several
independent responsibilities:

- older-page state merging;
- rendered chat assembly from snapshot plus older pages;
- visibility filtering for legacy "Working / Started" activity rows;
- legacy thought activity normalization;
- streamed Agent text coalescing;
- thought coalescing;
- adjacent activity coalescing and activity-run title/status derivation.

This module sits on the Task rendering path and is important for responsive chat
rendering, but it does not require Backend or App Shell changes. Splitting it is
a low-blast-radius way to keep Frontend state/view-model logic modular and
testable.

## Scope

- Keep `mergePageState(current, page)` importable from
  `state/chatPaging.ts`.
- Keep `renderedChat(snapshot, pageState)` importable from
  `state/chatPaging.ts`.
- Extract focused pure modules for page merging, item normalization/filtering,
  text/thought coalescing, and activity coalescing.
- Preserve all current rendering behavior, cursor selection, pending/error
  state, deduplication order, legacy thought conversion, filtered working row,
  and coalesced activity titles/statuses.

## Out Of Scope

- No reducer action changes.
- No `TaskView` or component rendering changes.
- No protocol/storage/chat schema changes.
- No visual redesign.
- No change to streaming coalescing heuristics.

## Risks

- Chat row deduplication order is easy to regress when splitting page merge
  helpers.
- Coalescing changes can alter visible Agent text, Thought, and activity rows.
- Cursor propagation for merged runs must continue using the last row cursor.
- Legacy activity normalization must happen before thought coalescing.

## Next Step

Record and commit the accepted API contract, then implement this slice.
