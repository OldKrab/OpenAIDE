# Frontend App Controller Assembly Split API Contract

Accept the Frontend App Controller Assembly split.

Public API:
- Keep `useAppController()` importable from `components/appController`.
- Keep the exported `AppController` type shape unchanged.
- Keep `createSnapshotRequestId` and `dispatch` exposed exactly as before.

Internal module contract:
- Extract controller mutable ref construction into a small hook owned by the
  controller layer. It may own `SnapshotRequestTracker`, native-session list
  request ids, latest request keys, and latest native-session selection.
- Extract native-session request posting into a focused controller helper that
  receives explicit dependencies and current selection data. It must keep
  request id incrementing, latest request id tracking, reducer dispatch, and
  `postNativeSessionList` payload shape unchanged.
- Extract pure derived controller view state into a shell-neutral module:
  active task lookup, visible task filtering, and active-task presence.
- `appController.ts` remains the only module that wires React effects,
  host-message session startup, timers, telemetry effects, reducer state, and
  callback factory assembly.

Behavior to preserve:
- Bootstrap preferences and agent initialization behavior.
- Snapshot request id creation and navigation-generation invalidation.
- Native-session list request ids, latest request tracking, append/start action,
  selected agent/workspace payload, cursor behavior, and host payload shape.
- Fallback agent selection when the selected new-task agent becomes disabled.
- Task-open fallback timer and active-task polling cadence.
- Task-rendered telemetry payload fields.
- Visible task filtering by trimmed lowercase query across title, agent name,
  and status.

Review focus:
- Ensure extracted helpers do not introduce hidden singleton state.
- Ensure request helpers receive current state explicitly instead of capturing
  stale selection values.
- Ensure the split does not move host effects, protocol access, timers, or
  product state ownership into rendering components.
- Add focused coverage for derived visible task state and native-session request
  helper behavior, while preserving existing mounted lifecycle tests.

