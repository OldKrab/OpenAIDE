# P417 - Non-Agent Settings Frontend Consumption

## Result

Wired Frontend Settings to consume App Server-owned MCP server and Skills projections.

## Changes

- Added protocol-to-frontend mapping for Settings sections, MCP server records, and Skill records.
- Added central Settings projection intent loading for Agent details, MCP servers, and Skills.
- Stored advertised Settings tabs and independent MCP/Skills loading, result, and error state in the reducer.
- Restored MCP and Skills tabs only when App Server advertises those sections.
- Rendered MCP/Skills loading, empty, error, and readonly record states.
- Updated startup and refresh flows to request all Settings projections through typed App Server methods.

## Verification

- `npm run check --workspace openaide-frontend`
- `npm run test --workspace openaide-frontend -- SettingsView appReducer appControllerCallbacks appController`
- `npm run build --workspace openaide-frontend`
- `git diff --check`
- Frontend production source-size guard, excluding tests

## Next

P418 should quickly select and implement the next concrete remaining architecture gap from the current code and refactor plan.
