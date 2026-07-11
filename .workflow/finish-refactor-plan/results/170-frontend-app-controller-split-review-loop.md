# Frontend App Controller Split: Review Loop

## Review Scope

Reviewed the working tree against fixed point `08fc3fa`
(`docs: refine frontend app controller callbacks`) using `$doomsday-review`.

## Passes

- Correctness subagent: no findings.
- Requirements/tests subagent: found missing executable coverage for moved
  controller host behavior.
- Code-quality subagent: no findings.
- Targeted requirements/tests rerun after fixes: no findings.

## Fixes

- Added `appControllerCallbacks.test.ts` to cover host-aware navigation,
  settings, new-task, task prompt, and permission callback contracts, including
  responsive pending dispatches before host requests where required.
- Added `appControllerEffects.ts` for named timing constants and lifecycle
  helper message construction.
- Added `appControllerEffects.test.ts` to cover startup requests, telemetry
  payloads, task-open fallback messages, active task polling messages, and the
  accepted 1200 ms fallback and 600 ms polling constants.
- Added `appController.test.tsx` with a mounted hook test harness to prove
  `useAppController()` actually starts navigation requests, schedules the task
  fallback, emits `task_rendered`, and installs active task polling.
- Added `react-test-renderer` as a Frontend test-only dev dependency for the
  mounted hook coverage.

## Verification During Review

- `npm run check --workspace openaide-frontend`
- `npm run test --workspace openaide-frontend -- App.test.tsx hostMessageRouter.test.ts appReducer.test.ts appController.test.tsx appControllerCallbacks.test.ts appControllerEffects.test.ts`
- `git diff --check`
- Source-size scan remains below the production source-file limit.
- Boundary scan remains clean for `AppSurfaces.tsx`, `NewTaskView.tsx`, and
  `TaskView.tsx`.

## Result

All material review findings are resolved. The slice is ready for integration
verification.
