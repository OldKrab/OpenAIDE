# Frontend Standalone Dev Host Split Implementation

Implemented the accepted Frontend Standalone Dev Host split only.

## Changes

- Kept `services/devHost.ts` as the public facade exporting
  `standaloneBootstrap()` and `createStandaloneHost()`.
- Added `devHostData.ts` for browser-free demo data factories.
- Added `devHostBootstrap.ts` for standalone preview bootstrap/path mapping.
- Added `devHostRouter.ts` for typed standalone host message routing through
  injected post/navigation outputs.
- Added `devHost.test.ts` for bootstrap mapping, invalid-message ignores,
  response metadata preservation, and surface navigation routing.

## Preserved Seams

- `hostBridge.ts` still imports only from `services/devHost.ts`.
- Standalone host public exports are unchanged.
- Demo response message types, payload shapes, request metadata, and route
  paths are preserved.
- Browser globals are wired only in `devHost.ts`; data and router modules are
  pure/injected.

## Preliminary Verification

- `npm run check --workspace openaide-frontend`
- `npm run test --workspace openaide-frontend -- devHost.test.ts`
- Browser-global boundary scan for new internal dev-host modules
- Source-size scan for changed production files

## Next Step

Run `$doomsday-review` for the implementation and fix material findings.
