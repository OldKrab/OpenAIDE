# Frontend Chat Paging Split API Contract

## Decision

Accept the Frontend Chat Paging split.

The external seam remains:

```ts
mergePageState(current, page): ChatPageState
renderedChat(snapshot, pageState): RenderedChat
```

`TaskView` and reducers continue importing only from `state/chatPaging.ts`.
No production caller should import the new implementation modules directly.

## Public Exports To Preserve

`state/chatPaging.ts` keeps exporting:

- `RenderedChat`
- `mergePageState`
- `renderedChat`

No new package entry points are added.

## Internal Modules

Create focused private modules under `packages/frontend/src/state/`:

- `chatPageMerge.ts`
  - message-row deduplication and older-page state merging;
  - cursor fallback for older-page state.

- `chatItemNormalization.ts`
  - visibility filtering for legacy "Working / Started" rows;
  - legacy Thought activity detection and conversion.

- `chatTextCoalescing.ts`
  - adjacent Agent text coalescing;
  - adjacent Thought coalescing;
  - text run streaming/cursor propagation.

- `chatActivityCoalescing.ts`
  - adjacent activity coalescing;
  - activity-run title and status derivation;
  - command and terminal-input activity classification.

If implementation shows fewer modules are cleaner, names may vary, but the same
responsibilities must remain separated and `chatPaging.ts` must stay the public
compatibility facade.

## Behavior To Preserve

- `mergePageState` appends existing older items after the new page items while
  deduplicating by `message_id` and preserving first-seen order.
- `mergePageState` sets `hasBefore` from `page.has_before`, `startCursor` from
  `page.start_cursor` or the first merged older row cursor, and `pending` to
  false.
- `renderedChat` merges older rows before snapshot rows with the same
  deduplication semantics.
- Rendered `hasBefore`, `beforeCursor`, `pending`, and `error` values remain
  exactly as before.
- Legacy `Working` activity rows with one text step equal to `Started` remain
  hidden.
- Legacy Thought activities remain converted to `thought` rows before thought
  coalescing.
- Agent text coalescing remains gated by the existing tiny-part and boundary
  heuristic.
- Coalesced Agent text and Thought rows keep the first row identity/message
  fields, use the last row cursor, concatenate text in order, and preserve
  streaming when any row is streaming.
- Adjacent activity runs keep the first row identity/message fields, use the
  last row cursor, flatten steps in order, derive status as error over running
  over completed, collapse only when every activity is collapsed, and derive
  titles as `Commands`, `Terminal input`, or `Tool activity` with the current
  classification rules.

## Boundary Rules

- New modules must be pure and shell-neutral: no React, browser APIs, host
  bridge, reducers, App Server client imports, timers, or mutation of input
  arrays.
- `chatPaging.ts` remains the only production import path for chat paging APIs.
- Internal helper exports should be only what another split module or tests
  need; avoid creating a broad public-looking helper surface.
- Do not change `ChatMessage`, `MessagePage`, `TaskSnapshot`, or
  `ChatPageState` contracts.

## Tests

Preserve existing reducer/rendered-chat tests and add focused helper-level or
facade-level coverage where behavior is not already pinned.

Required coverage:

- Page merge deduplicates by `message_id` and preserves first-seen order.
- `renderedChat` filters legacy `Working / Started` rows.
- Legacy Thought activity converts before thought coalescing.
- Agent text coalescing preserves first identity, last cursor, concatenated
  text, and streaming flag.
- Adjacent activity coalescing preserves flattened step order, last cursor,
  status precedence, collapse semantics, and title classification.

## Out Of Scope

- No UI/component changes.
- No chat schema changes.
- No new coalescing heuristics.
- No Backend or App Shell changes.
