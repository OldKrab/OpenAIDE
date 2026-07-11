# A5e Frontend Protocol State Mapping

Goal: add a shell-neutral transitional mapper from App Server Protocol Task
state into the current Frontend render-state contracts before migrating
`task/open` and `task/list` intents.

## Scope

- Added `mapProtocolTaskNavigation(...)` for App Server task navigation
  snapshots, preserving mapped legacy task rows and `activeTaskId`.
- Added `mapProtocolTaskSnapshot(...)` for App Server task snapshots, including
  chat item mapping, config option values, conservative legacy status mapping,
  and explicit `requiresNativeSurface` gating for states the legacy UI cannot
  honestly support.
- Added protocol chat mapping helpers for user, agent, activity, interrupted,
  pending-request, and recovery fallback rows.
- Kept raw legacy `workspace_root` empty when the App Server Protocol exposes
  only project labels, preventing safe display labels from being used as paths.
- Added warnings for lossy transitional cases such as pending App Server
  requests, recovery actions, preparation/send blockers, command readiness,
  missing agent labels, and unmapped project display.

## Boundaries

- This slice does not wire the mapper into App Controller or migrate any
  product intent yet.
- Pending App Server requests and recovery actions are not treated as fully
  actionable legacy UI. They mark `requiresNativeSurface` so the next migration
  step can avoid pretending the fallback is complete.
- App Server Protocol remains the source of truth; the mapper is transitional
  glue for the existing legacy Frontend render contracts.

## Verification

- `npm run test --workspace openaide-frontend -- appServerProtocolMapping.test.ts` passed.
- `npm run check --workspace openaide-frontend` passed.
- `npm run test --workspace openaide-frontend` passed.
- `git diff --check` passed.
- Changed production source files are below the split threshold.

## Next

Continue A5 by wiring the typed BackendConnection request path into
`task/open` and `task/list`, using this mapper only for legacy-safe snapshots
and routing native-surface-required states through explicit follow-up handling.
