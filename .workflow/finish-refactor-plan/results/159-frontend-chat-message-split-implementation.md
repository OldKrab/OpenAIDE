# Frontend Chat Message Split: Implementation

## Scope Implemented

Implemented the accepted Frontend Chat Message split without changing public
`ChatRow` props, rendered chat behavior, host/runtime contracts, CSS classes,
or visible chat copy.

## Code Changes

- Kept `ChatMessageView.tsx` as the public `ChatRow` component:
  - message-kind routing;
  - user, agent text, thought, activity, interruption, and permission row
    dispatch;
  - `firstToolPath` re-export.
- Added `chatMessageActions.tsx` for copy button UI and clipboard fallback.
- Added `ChatActivityView.tsx` for activity step rows, lazy tool-detail loading,
  and activity group rendering.
- Added `ChatToolDetailsView.tsx` for read/edit/search/execute/generic
  tool-detail rendering.
- Added `ChatToolBlocks.tsx` for reusable tool metadata, fields, content,
  code, search results, diff blocks, and the `ToolPath` open-file button.
- Added `ChatPermissionCard.tsx` for permission rendering and allow/deny
  option mapping.
- Added `chatToolIcons.tsx` for neutral local tool-kind icon mapping shared by
  activity rows and permission cards.
- Added focused tests for the extracted lazy-load predicate, open-path payload
  construction, and permission decision mapping.

## Contract Adjustment

The accepted contract said `ToolPath` lived in `ChatToolDetailsView.tsx`. During
implementation, `ToolPath` moved into `ChatToolBlocks.tsx` to keep
`ChatToolDetailsView.tsx` under the production source-file size limit. The
ownership rule is unchanged: `ToolPath` remains the only component in this split
that imports `postHostMessage`, and it only sends the existing `tool.openPath`
payload.

## Verification Before Review

- `npm run check --workspace openaide-frontend`
- `npm run build --workspace openaide-frontend`
- `npm run test --workspace openaide-frontend -- ChatMessageView.test.tsx`
- `npm run check`
- `git diff --check`

All checks passed before review.

## Source File Size Check

- `ChatToolDetailsView.tsx`: 285 lines.
- `ChatToolBlocks.tsx`: 142 lines.
- `ChatActivityView.tsx`: 142 lines.
- `ChatPermissionCard.tsx`: 131 lines.
- `ChatMessageView.tsx`: 93 lines.
- `chatMessageActions.tsx`: 39 lines.
- `chatToolIcons.tsx`: 10 lines.

All Chat message production source files are under the project source-file size
limit.
