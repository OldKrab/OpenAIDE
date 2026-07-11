# Frontend App Controller Assembly Split Implementation

Implemented the accepted Frontend App Controller Assembly split only.

Changed modules:
- `appController.ts` remains the public `useAppController` facade and still
  wires React effects, host-message session startup, timers, telemetry effects,
  reducer state, and callback factory assembly.
- `appControllerRefs.ts` owns controller-local mutable ref construction.
- `appControllerNativeSessions.ts` owns native-session request id tracking,
  loading dispatch, and host request posting behind explicit dependencies.
- `appControllerDerivedState.ts` owns pure active-task and visible-task
  derivation.

Focused verification before review:
- `npm run check --workspace openaide-frontend`
- `npm run test --workspace openaide-frontend -- appControllerAssembly.test.ts appController.test.tsx appControllerCallbacks.test.ts appControllerEffects.test.ts`

