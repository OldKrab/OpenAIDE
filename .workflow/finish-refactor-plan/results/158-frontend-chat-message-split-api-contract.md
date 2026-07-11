# Frontend Chat Message Split: API Contract

## Accepted Contract

Split `ChatMessageView.tsx` into local shell-neutral Chat message rendering
modules without changing rendered output, behavior, state ownership, or
App Shell/App Server contracts.

## Module Layout

Create focused modules under `packages/frontend/src/components/`:

- `ChatMessageView.tsx`
  - public `ChatRow` component;
  - message-kind routing for user, agent text, thought, activity,
    interruption, and permission rows;
  - existing `firstToolPath` re-export.
- `chatMessageActions.tsx`
  - `MessageCopyAction`;
  - clipboard fallback implementation.
- `ChatActivityView.tsx`
  - activity group rendering;
  - activity step row rendering;
  - lazy tool-detail load trigger;
  - activity/tool icon selection.
- `ChatToolDetailsView.tsx`
  - typed read/edit/search/execute tool-detail rendering;
  - generic tool-detail composition.
- `ChatToolBlocks.tsx`
  - generic tool metadata, fields, content, code, search results, and diff
    rendering;
  - `ToolPath` open-file button that owns the existing `postHostMessage`
    `tool.openPath` call.
- `ChatPermissionCard.tsx`
  - permission-card rendering;
  - permission option decision mapping;
  - resolved/responding/approval-required presentation.

No reducer, state store, tool-detail view-model, activity label, host bridge,
contract type, CSS, or generated file changes are introduced in this slice.

## Component API

`ChatRow` keeps its current public props exactly:

- `message`;
- `onLoadToolDetail`;
- `onPermissionRespond`;
- `permissionResponse`;
- `taskId`;
- `toolDetails`.

`ChatActivityView` receives only:

- activity body;
- `taskId`;
- `toolDetails`;
- `onLoadToolDetail`.

`ChatToolDetailsView` receives only:

- tool `step`;
- resolved or inline `details`;
- lazy-load `loading` and `error` state;
- fallback preview.

`ChatPermissionCard` receives only:

- normalized permission message body;
- response state;
- `onRespond`.

## Ownership Rules

- `ChatRow` remains the only public chat row entry point used by parent
  components.
- `ChatRow` owns only message-kind routing and simple user/agent/thought row
  composition.
- `ChatActivityView` owns activity disclosure structure and lazy detail-loading
  conditions, but not tool-detail normalization or activity label policy.
- `ChatToolDetailsView` owns typed tool-detail composition and may import
  `toolDetailsViewModel` helpers. It must not mutate state, call reducers, or
  own product normalization.
- `ChatToolBlocks` owns reusable tool-detail blocks. `ToolPath` in this module
  is the only component in this split allowed to import `postHostMessage`, and
  only for the existing `tool.openPath` action.
- `ChatPermissionCard` owns permission presentation and local option-to-decision
  mapping only. It must not import host bridge, App Server client bindings, or
  reducer actions.

## Behavior Invariants

This slice must preserve:

- all visible chat/activity/tool/permission text;
- all CSS class names;
- ARIA labels and disclosure structure;
- user and agent copy-button placement and labels;
- clipboard fallback behavior;
- user attachment rendering;
- thought row rendering;
- activity group default open/collapsed behavior;
- lazy tool-detail loading condition:
  open row, `detail_artifact_id` present, details absent, not loading, and no
  previous error;
- read, edit, search, execute, and generic tool-detail rendering;
- tool path open behavior and `tool.openPath` payload shape;
- search highlighting and caret rendering;
- permission status labels, button classes, disabled states, and allow/deny
  mapping;
- OpenCode `external_directory` permission special display;
- `firstToolPath` re-export.

## Out Of Scope

- No visual redesign.
- No copy changes.
- No activity label or tool-detail view-model changes.
- No reducer/action/state changes.
- No App Shell or App Server Protocol changes.
- No CSS class renames.
- No generated file edits.

## Review Requirements

`$doomsday-review` must check at least:

- `ChatRow` still exposes the same public API and rendered behavior;
- lazy tool-detail loading has the same trigger conditions;
- only tool-path opening imports `postHostMessage`;
- permission decisions and disabled states are unchanged;
- `ChatMessageView.test.tsx` still validates the public rendered markup;
- production Chat message source files are under the project source-file size
  limit.

## Verification Plan

Run:

- `npm run check --workspace openaide-frontend`
- `npm run build --workspace openaide-frontend`
- `npm run test --workspace openaide-frontend -- ChatMessageView.test.tsx`
- `npm run check`
- `git diff --check`

Also run a source file size check for production Frontend files and the updated
Chat message modules.
