# Frontend Sidebar Split Review Loop

Ran `$doomsday-review` for the Frontend Sidebar split with subagents for
correctness, requirements/tests, and code quality.

Initial findings:
- Requirements/tests found missing top-level `Sidebar` facade coverage for
  archive hiding native sessions, native-session errors with valid rows,
  load-more cursor behavior, and load-more disabled states.
- Code quality found that `sidebarViewModel` still typed its input through the
  root `AppState` store shape.

Fixes:
- Added `Sidebar` facade tests for archive gating, native-session error
  rendering alongside valid rows, load-more cursor dispatch, and load-more
  disabled behavior while loading or adopting.
- Replaced the root-store indexed view-model input type with a narrow local
  `SidebarNativeSessionListState` contract.

Rerun result:
- Correctness: no findings.
- Requirements/tests: no findings.
- Code quality: no findings.

