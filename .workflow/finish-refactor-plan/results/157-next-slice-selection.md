# Next Slice Selection: Frontend Chat Message Split

## Decision

Select the Frontend Chat Message split as the next refactor slice.

## Why This Slice

`packages/frontend/src/components/ChatMessageView.tsx` is 777 lines and is now
the largest production Frontend source file. It mixes several separable
responsibilities:

- public `ChatRow` message-kind routing;
- user, agent, thought, activity, interruption, and permission message
  rendering;
- copy action and clipboard fallback;
- activity-step rendering and lazy tool-detail loading;
- typed read/edit/search/execute tool-detail rendering;
- generic tool metadata/content rendering;
- tool-path open routing through the host bridge;
- permission card rendering and response decision mapping.

This slice continues the Frontend split by turning a large rendering file into
focused shell-neutral components while preserving the current host bridge and
tool-detail view-model boundaries.

## Slice Boundary

Split `ChatMessageView.tsx` into local Chat message components under
`packages/frontend/src/components/`:

- keep `ChatMessageView.tsx` as the public `ChatRow` component and message-kind
  router;
- move copy/fallback clipboard UI into a small message-action helper module;
- move activity-step and tool-detail UI into focused Activity/Tool modules;
- move permission-card UI into a permission-focused module;
- preserve the existing `firstToolPath` re-export for callers;
- preserve `postHostMessage` path-open behavior and keep it inside the
  tool-path UI module;
- preserve all visible text, CSS class names, ARIA labels, lazy detail-loading
  behavior, and permission response behavior.

## Out Of Scope

- No chat UI redesign.
- No copy changes.
- No reducer/action/state changes.
- No tool-detail view-model changes.
- No App Shell or App Server Protocol changes.
- No CSS class renames.
- No generated file edits.

## Risk Map For API Grill

- `ChatRow` must remain the only public row entry point used by `TaskView`.
- Tool rendering modules may import tool-detail view-model helpers, but must
  not take over product normalization or activity label policy.
- The tool-path component may use `postHostMessage` for `tool.openPath`, but
  message-kind routing and permission UI must not import host bridge services.
- Lazy loading must still call `onLoadToolDetail` only when a details row opens
  and details are absent, not loading, and not failed.
- Permission response mapping must remain limited to allow/deny options and
  preserve disabled behavior for non-decision options.
- Existing tests in `ChatMessageView.test.tsx` must keep proving externally
  rendered markup rather than internal module boundaries.
