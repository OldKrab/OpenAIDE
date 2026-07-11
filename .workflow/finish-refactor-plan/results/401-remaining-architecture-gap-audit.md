# P401 Remaining Architecture Gap Audit

## Result

- Confirmed the Custom Agent replacement cleanup packet landed cleanly.
- Audited active source and plan text for remaining legacy product bridges.
- Selected the next concrete cleanup: remove dead shared Frontend `settings:result` / `SettingsSnapshot` state.

## Evidence

Active source no longer emits shell `settings.snapshot`, but Frontend still has:

- `settings:result` action in `appReducer` / `settingsReducer`
- `SettingsState.snapshot`
- fallback lookups in Agent Settings helpers
- reducer tests that maintain stale Settings snapshot behavior
- `SettingsView` props for a full legacy settings snapshot

## Next Packet

P402 should remove the dead legacy Settings snapshot branch from shared Frontend state and tests. Settings state should be composed from:

- Backend Agent Settings details
- Backend App Preferences
- Backend Runtime Settings
- shell-only developer unlock capability where still needed

## Verification For P402

- `npm run check --workspace openaide-frontend`
- `npm run test --workspace openaide-frontend -- appReducer.test.ts SettingsView.test.tsx AgentSettingsTab.test.tsx appControllerCallbacks.test.ts appController.test.tsx`
- Active source scan for `settings:result`, `SettingsState.snapshot`, and `settings.snapshot` outside tests that assert no legacy fallback.
