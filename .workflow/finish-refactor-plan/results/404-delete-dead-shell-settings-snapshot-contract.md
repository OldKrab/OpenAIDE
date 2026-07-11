# P404 delete dead shell Settings snapshot contract

## Result

Deleted the unused transitional shell Settings snapshot surface left behind after shared Frontend stopped storing `SettingsSnapshot`.

## Implementation

- Removed `SettingsSnapshot`, `CommonSettingsRecord`, and `DeveloperSettingsRecord` from `packages/app-shell-contracts/src/webview/settings.ts`.
- Kept still-used diagnostics, workspace-root, Agent, MCP, and Skill record types.
- Deleted unreachable Frontend MCP and Skills Settings panel components.
- Updated the refactor plan and workflow state.

## Verification

- `npm run build --workspace @openaide/app-shell-contracts`
- `npm run check --workspace openaide-frontend`
- `npm run check --workspace openaide-vscode-extension`
