# Next Slice Selection: Frontend App Controller Assembly Split

Select the Frontend App Controller Assembly split as the next refactor slice.

Reasoning:
- `packages/frontend/src/components/appController.ts` remains a broad hook module
  that combines controller state, mutable request/session refs, request helper
  construction, host-message session startup, effects, callbacks, telemetry, and
  derived view state.
- Previous Frontend slices already extracted callbacks, effects helpers, chat
  paging, dev host, and sidebar rendering; the controller hook is now the next
  useful facade to slim without changing product behavior.
- A narrow split can reduce controller coupling while preserving the existing
  lifecycle and host-message timing behavior.

Out of scope:
- No host-message protocol changes.
- No timer/polling cadence changes.
- No callback API changes.
- No AppSurfaces or UI rendering changes.
- No rewrite of effect ordering or dependency strategy.

