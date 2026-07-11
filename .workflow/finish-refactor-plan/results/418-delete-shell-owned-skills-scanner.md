# P418 - Delete Shell-Owned Skills Scanner

## Result

Removed the obsolete VS Code shell-owned Skills scanner after MCP/Skills Settings moved to App Server-owned projections.

## Changes

- Deleted the self-contained VS Code Skills scanner, metadata parser, discovery helpers, record helpers, types, and tests.
- Verified no remaining imports or references to the deleted scanner exist.

## Verification

- `rg` for deleted scanner symbols and modules
- `npm run check --workspace openaide-vscode-extension`
- `npm run test --workspace openaide-vscode-extension`
- `git diff --check`
- VS Code extension production source-size guard, excluding tests

## Next

P419 should fast-pick the next concrete stale shell/product boundary from the current codebase and close it in the smallest safe slice.
