# Next Slice Selection: Frontend Tool Details Split

## Decision

Select the Frontend Tool Details split as the next refactor slice.

## Why This Slice

`packages/frontend/src/state/toolDetailsViewModel.ts` is now the largest
Frontend state module and mixes several independent responsibilities:

- generic tool-detail availability and path/output helpers;
- read/edit/search/execute-specific view-model construction;
- shell-command normalization;
- unified diff generation;
- search-output parsing and path normalization;
- CSS-safe tool-kind classification.

`packages/frontend/src/components/ChatToolDetailsView.tsx` also mixes the
public `ChatToolDetails` router with concrete read, edit, search, and execute
renderers. Splitting this cluster continues the Frontend refactor without
changing App Shell contracts, host messages, reducer state, or App Server
protocol boundaries.

## Scope

- Keep `ChatToolDetails` as the public tool-details renderer used by
  `ChatActivityView`.
- Keep `ChatToolBlocks.tsx` reusable primitives stable for callers.
- Extract focused view-model modules for:
  - generic tool-detail helpers;
  - command/output helpers;
  - diff/edit helpers;
  - search helpers;
  - execute helpers.
- Extract focused renderer modules for read, edit, search, execute, and
  generic tool details if needed to keep component files small.
- Preserve all visible text, CSS classes, ARIA labels, icon choices, path-open
  host messages, fallback-preview behavior, loading/error behavior, and
  existing tests.

## Out Of Scope

- No App Shell message changes.
- No host bridge changes.
- No reducer, state shape, or cache-key changes.
- No App Server protocol work.
- No visual redesign.

## Risks

- Search rendering has path normalization and match-highlighting behavior that
  is easy to accidentally change.
- Execute output fallback order differs for failed and successful runs.
- Edit result text depends on failure state, stderr, fallback preview, and
  whether a diff represents a new file.
- `firstToolPath` is re-exported from public files and must remain stable.

## Next Step

Record and commit the accepted API contract, then implement this slice.
