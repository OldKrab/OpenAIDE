# Frontend Sidebar Split Implementation

Implemented the accepted Frontend Sidebar split only.

Changed modules:
- `Sidebar.tsx` remains the public composition facade for the task-navigation
  aside.
- `sidebarViewModel.ts` owns pure visible native-session, visible-count, and
  empty-state derivation behind a narrow local input contract.
- `SidebarTaskRow.tsx` owns task open/archive/restore row rendering.
- `SidebarNativeSessionRow.tsx` owns listed native-session open/adoption
  disabled row rendering.
- `SidebarRowParts.tsx` owns sidebar-local row metadata and action-slot helpers.

Focused verification before review:
- `npm run check --workspace openaide-frontend`
- `npm run test --workspace openaide-frontend -- Sidebar.test.tsx AppSurfaces.test.tsx`

