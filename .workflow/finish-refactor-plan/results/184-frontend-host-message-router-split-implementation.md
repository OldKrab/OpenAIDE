# Frontend Host Message Router Split Implementation

## Scope

Implemented the accepted Frontend Host Message Router split only.

## Changes

- Kept `routeHostMessage(message, context)` as the public App Shell-message ingress.
- Kept `HostMessageRouterContext` and `sendWebviewTelemetry` importable from `hostMessageRouter.ts`.
- Added `hostMessageRouterTypes.ts` for router context and route types.
- Added `hostMessageTelemetry.ts` for webview telemetry posting.
- Added focused domain routers:
  - `hostSettingsMessages.ts`
  - `hostAgentSessionMessages.ts`
  - `hostNavigationMessages.ts`
  - `hostTaskMessages.ts`
  - `hostRuntimeErrorMessages.ts`
- Preserved routing order, stale result guards, snapshot telemetry, native-session pagination follow-up, dispatch actions, posted shell messages, and runtime error fallback messages.
- Extended `hostMessageRouter.test.ts` through the public router seam for native-session pagination, archive stale filtering, accepted snapshot side effects, and runtime-error stale/settings behavior.

## Preliminary Verification

- `npm run check --workspace openaide-frontend`
- `npm run test --workspace openaide-frontend -- hostMessageRouter.test.ts`
- `npm run check`
- `npm test -- --runInBand`
- `git diff --check`
- Host message router source-size scan
- Host message router boundary scan for rendering, host bridge, App Server client, app controller, and settings UI imports
