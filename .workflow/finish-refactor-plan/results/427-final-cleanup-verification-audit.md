# P427 Final Cleanup Verification Audit

## Status

Completed.

## Audit Result

The audit did not find active Frontend product bridge fallbacks for the old
task, session, permission, settings, or Agent shell-message paths. Remaining
matches were either historical plan notes, product-safe fallback display text,
or internal helper names in the typed App Server mapping layer.

## Implementation

Removed stale `legacy*` naming from typed Frontend mapping helpers:

- `legacyTaskStatus` -> `taskSummaryStatusFromProtocol`
- `legacyConfigCategory` -> `configCategoryFromProtocol`
- `legacyChatMessage` -> `chatMessageFromProtocol`
- `legacyActivityStatus` -> `activityStatusFromProtocol`
- `LEGACY_DEFAULT_ISOLATION` -> `DEFAULT_LOCAL_ISOLATION`

No runtime behavior changed.

## Verification

Passed:

- `npm run test --workspace openaide-frontend -- appServerProtocolMapping appServerProtocolChatMapping`
- `npm run check --workspace openaide-frontend`

Also audited with:

- `rg "legacy|fallback|task\\.list|task\\.snapshot|session\\.prompt|permission\\.respond|settings\\.snapshot|agent\\.listSessions|agent\\.configOptions|task\\.create|task\\.cancel|task\\.archive|task\\.restore" packages/frontend/src apps/vscode-extension/src packages/app-shell-contracts/src -n --glob '!**/*.test.ts' --glob '!**/*.test.tsx'`
- `rg "todo!|unimplemented!|TODO|FIXME|panic!\\(" openaide-rs/app-server/src openaide-rs/app-server-protocol/src packages/app-server-client/src packages/frontend/src apps/vscode-extension/src -n --glob '!**/*.test.*' --glob '!**/generated/*'`

## Next Packet

P428 should perform a requirement-by-requirement completion audit against the
refactor plan, current code, and available verification evidence before
deciding whether the active goal can be marked complete.
