# 302 Agent Settings Status Ownership

## Scope

Next A6 slice: remove VS Code-owned Agent probing and status classification from
the legacy Settings snapshot route.

## Contract

- `settings.snapshot` remains a shell fallback for non-Agent Settings sections.
- Agent Settings rows in that fallback must come from App Server Protocol
  `settings/getAgentDetails`, not from VS Code runtime `agent.probe`.
- VS Code must not classify Agent probe failures into product statuses.
- If App Server Agent details are temporarily unavailable, non-Agent Settings
  stay renderable and Agent rows return empty for that fallback response.

## Non-Goals

- No Frontend Settings redesign.
- No Agent probe API removal.
- No runtime restart policy change.
- No App Server status persistence beyond the existing status cache.

## Implementation Result

- `collectSettingsSnapshot` now requests `settings/getAgentDetails` through
  `RuntimeClient.appServerRequest`.
- Removed VS Code-side Agent probe/status/error mapping from Settings snapshot
  collection.
- Removed unused agent store/secret inputs from the Settings webview route.
- Replaced probe-oriented Settings snapshot tests with App Server-owned Agent
  details tests.

## Verification

- `$doomsday-review` correctness, requirements/tests, and code-quality passes;
  material icon-mapping finding fixed.
- `npm run check --workspace openaide-vscode-extension`
- `npm test --workspace openaide-vscode-extension -- src/settings/snapshot.test.ts src/webview/messaging.test.ts`
- `jq empty .workflow/finish-refactor-plan/state.json`
- `git diff --check`
- VS Code production source-size scan for files over 300 lines

## Next

Select and grill the next A6 slice.
