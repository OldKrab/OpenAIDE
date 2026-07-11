# Frontend Tool Details Split API Contract

## Decision

Accept the Frontend Tool Details split.

The external rendering seam remains:

```ts
<ChatToolDetails step={step} details={details} fallbackPreview={preview} loading={loading} error={error} />
```

`ChatActivityView` remains the production caller. Rendering modules outside the
tool-details cluster must not import the new specialized renderer modules.

## Public Exports To Preserve

`ChatToolDetailsView.tsx` keeps exporting:

- `ChatToolDetails`

`ChatToolBlocks.tsx` keeps exporting:

- `ToolOpenPathMessage`
- `ToolInputDetails`
- `ToolMeta`
- `ToolFields`
- `ToolContentBlock`
- `ToolPath`
- `toolOpenPathMessage`
- `ToolCodeBlock`
- `SearchOutput`
- `hasToolInput`

Public compatibility exports remain available from their current import paths:

- `firstToolPath` through `ChatMessageView.tsx` and `App.tsx`
- existing imports from `state/toolDetailsViewModel.ts` used outside the
  extracted modules

## Internal Modules

Split `packages/frontend/src/state/toolDetailsViewModel.ts` into focused
state/view-model helpers:

- `toolDetailsViewModel.ts`
  - public compatibility barrel for existing callers;
  - generic `hasToolDetails`, `toolKindClass`, `firstToolPath`, output-field
    filtering, primary output, path reading, and path normalization helpers
    only if they are genuinely shared.
- `toolCommandViewModel.ts`
  - `displayCommand`, shell-launcher recognition, and shell output line
    normalization helpers.
- `toolEditViewModel.ts`
  - unified diff generation, edit diff rows, first diff lookup, and edit
    result text.
- `toolSearchViewModel.ts`
  - search detail info, search result parsing, file-result parsing,
    query/path extraction, caret-line calculation, and openable path handling.
- `toolExecuteViewModel.ts`
  - execute detail info and failed/running/completed output classification.

If implementation shows a smaller split is cleaner, the module names may vary,
but the same responsibilities must remain separated and the compatibility
imports must stay stable.

Split `packages/frontend/src/components/ChatToolDetailsView.tsx` into focused
renderers while keeping `ChatToolDetailsView.tsx` as the public router:

- `ReadToolDetails.tsx`
- `EditToolDetails.tsx`
- `SearchToolDetails.tsx`
- `ExecuteToolDetails.tsx`
- `GenericToolDetails.tsx`

Renderer modules may import view-model helpers and `ChatToolBlocks` primitives.
They must not import host bridge APIs directly; path opening remains owned by
`ToolPath` in `ChatToolBlocks.tsx`.

## Behavior To Preserve

- Missing details render loading text, error text, fallback preview, or nothing
  exactly as before.
- Tool-name routing remains `read`, `edit`, `search`, `execute`, then generic
  fallback.
- Read details preserve path display, numbered output lines, empty-output text,
  and shell command display.
- Edit details preserve file metadata, hunk label, diff line numbering,
  fallback preview, empty diff text, result text, failure state, and icons.
- Search details preserve failed, empty, matched, and file-list states; command
  not found cleanup; match highlighting; caret-line rendering; file opening
  paths; and fallback preview behavior.
- Execute details preserve running, failed, and completed states; command chip;
  output fallback priority; output labels; duration text; exit-code chip; and
  icons.
- Generic details preserve tool metadata, input details, content blocks, search
  output special case, stderr rendering, filtered output fields, and fallback
  preview behavior.
- `ToolPath` continues posting the same `tool.openPath` message payload.
- `firstToolPath` and `parseSearchResults` behavior remains stable for public
  callers and tests.

## Boundary Rules

- View-model modules must be pure and shell-neutral: no React, host bridge,
  DOM/browser APIs, timers, reducers, or App Server client imports.
- Renderer modules may depend on React, icons, `ChatToolBlocks`, and pure
  view-model helpers only.
- Host message posting remains isolated to `ToolPath`.
- Do not introduce new package entry points or barrel exports beyond preserving
  current compatibility paths.
- Do not change tool-detail data contracts from `@openaide/app-shell-contracts`.

## Tests

Extend focused tests through public renderers and public helper exports.

Required coverage:

- Existing typed read/edit/search/execute rendering remains green.
- Search match parsing still supports `path:line:text` and
  `path:line:column:text` forms.
- Search file-list mode still opens relative files against `cwd`.
- Execute failed output prefers stderr before aggregated/formatted/stdout
  fallback.
- Edit result text preserves created/updated and failed fallback behavior.
- `firstToolPath` returns location, diff path, input path, then undefined.
- `ToolPath` host message payload remains unchanged.

## Out Of Scope

- No visual redesign.
- No class name or visible text changes.
- No App Shell or host message changes.
- No reducer/state cache behavior changes.
