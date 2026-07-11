# Frontend Sidebar Split API Contract

Accept the Frontend Sidebar split.

Public API:
- Keep `Sidebar` importable from `components/Sidebar`.
- Keep the existing `Sidebar` props and callback meanings unchanged.

Internal module contract:
- `Sidebar` remains the composition facade for the task-navigation aside.
- Extract pure sidebar view-model derivation into a shell-neutral module that
  computes visible native sessions, visible count, and empty-state copy from
  current props.
- Extract task-row rendering into a focused component module that owns task
  open/archive/restore row UI only.
- Extract native-session row rendering into a focused component module that owns
  listed native-session open/adoption disabled UI only.
- Shared row metadata/action-slot helpers must stay local to sidebar components
  and must not become general app abstractions until another caller exists.

Behavior to preserve:
- Search trims and lowercases query text before native-session filtering.
- Archive mode shows archived tasks only and hides listed native sessions.
- Non-archive mode counts both tasks and visible native sessions.
- Native-session errors render without hiding valid rows.
- Adoption disables listed-session open actions while any adoption is pending.
- Load-more uses the current native-session cursor and is disabled while loading
  or adopting.
- Task row selected/read/unread/status classes and archive/restore labels remain
  unchanged.

Review focus:
- Ensure the split does not move product state ownership into rendering
  components.
- Ensure extracted modules stay Frontend-only and do not import App Server,
  transport, browser globals, timers, or mutable singleton state.
- Add focused coverage for view-model behavior and row callback/disabled
  behavior that was previously only implicit.

