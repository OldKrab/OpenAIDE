# P420 - Trim Shell Agent Settings Stubs

## Result

Removed dead VS Code shell-owned Agent Settings catalog and mutation stubs. The shell keeps only the secret-key helper still needed for Backend-initiated Agent secret requests.

## Changes

- Trimmed `settings/agents.ts` to `customAgentSecretKey`.
- Deleted obsolete Agent Settings shell tests.
- Removed unused `agentSettingsStore` plumbing from webview message contexts.
- Removed obsolete messaging test mocks for shell-owned Agent mutations.

## Verification

- `rg` for deleted shell Agent Settings symbols and plumbing
- `npm run check --workspace openaide-vscode-extension`
- `npm run test --workspace openaide-vscode-extension`
- `git diff --check`
- VS Code extension production source-size guard, excluding tests

## Next

P421 should fast-pick the next stale shell/frontend product bridge or update stale plan wording if the code gap is already closed.
