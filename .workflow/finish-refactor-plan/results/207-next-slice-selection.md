# Next Slice Selection: Frontend Sidebar Split

Select the Frontend Sidebar split as the next refactor slice.

Reasoning:
- `packages/frontend/src/components/Sidebar.tsx` still combines sidebar shell layout,
  archive/search header behavior, task row rendering, native-session row rendering,
  empty-state derivation, pagination controls, and footer rendering.
- The file is below the hard source-file limit, but it is a natural Frontend
  component boundary with several independent responsibilities.
- A focused split can preserve the public `Sidebar` component API while making
  the reusable row and view-model pieces easier to test and review.

Out of scope:
- No visual redesign.
- No changes to App Server protocol, state shape, task filtering ownership, or
  native-session adoption behavior.
- No change to shell-specific APIs or routing.

