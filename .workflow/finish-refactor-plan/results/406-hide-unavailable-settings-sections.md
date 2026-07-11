# P406 hide unavailable Settings sections

## Result

Hid Settings tabs that do not have renderable App Server-owned projections yet.

## Implementation

- Settings currently renders only `Agents` and `General`.
- If stale state selects an unavailable tab such as `skills`, the view falls back to the Agents tab.
- Removed endless skeleton rendering for MCP/Skills sections when no loading work exists.
- Added a regression test that MCP/Skills tabs stay hidden until real projections are implemented.

## Verification

- `npm run check --workspace openaide-frontend`
- `npm run test --workspace openaide-frontend -- SettingsView.test.tsx`
