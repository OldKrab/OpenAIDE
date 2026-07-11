# P402 remove Frontend legacy Settings snapshot state

## Result

Removed the dead shared Frontend `settings:result` / `SettingsSnapshot` reducer path.

## Implementation

- Removed `snapshot` from `SettingsState` and the `settings:result` action from the reducer contract.
- Removed legacy snapshot merge helpers and all Frontend fallback reads from `state.settings.snapshot`.
- Made Settings rendering use typed Agent details, controller-owned app preferences, and Backend runtime settings.
- Left MCP and Skills Settings panels on skeleton presentation until App Server-owned projections exist.
- Updated reducer, Settings view, and controller tests to cover the new projection boundaries.

## Verification

- `npm run build --workspace @openaide/app-shell-contracts`
- `npm run check --workspace openaide-frontend`
- `npm run check --workspace openaide-vscode-extension`
- `npm run test --workspace openaide-frontend -- appReducer.test.ts SettingsView.test.tsx AgentSettingsTab.test.ts appControllerCallbacks.test.ts appController.test.tsx`
