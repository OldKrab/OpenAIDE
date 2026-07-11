# P414 non-Agent Settings projections API

## Result

Accepted the App Server-owned API shape for MCP and Skills Settings projections.

## Contract

- Add typed read methods:
  - `settings/getMcpServers`
  - `settings/getSkills`
- The methods return App Server-owned render records, not shell snapshots.
- Records expose only safe labels, scopes, status, descriptions, counts, warnings, generated timestamps, and safe notices.
- Records must not expose raw paths, secret values, shell picker metadata, or VS Code APIs.
- MCP and Skills are read-only in this slice. Mutations such as enable/edit/install need separate accepted APIs.
- `SettingsSnapshot.sections` advertises `mcpServers` and `skills` only after the matching method and projection source are implemented.
- Frontend lazy-loads sections through the central intent layer when the tab opens or refreshes and renders explicit loading/error/empty states.
- Skills scanning moves to Backend/App Server ownership using Backend-known global and project/workspace skill roots with safe source labels.
- Existing VS Code skill scanning code may be deleted or reused only as Backend-compatible helper logic, not as an App Shell product source.
- MCP projection reads from Backend-owned MCP configuration/state when that source exists; if no source exists, it returns a ready empty projection with optional safe notices.

## Next

Implement protocol source types and generated TypeScript bindings for the two read methods before wiring runtime projections.
