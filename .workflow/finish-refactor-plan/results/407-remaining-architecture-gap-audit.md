# P407 remaining architecture gap audit

## Result

Selected `P408-advertise-only-renderable-settings-sections` as the next packet.

## Finding

The shared Frontend now hides MCP and Skills Settings tabs until App Server-owned projections exist, but `SettingsCatalog` still advertises `mcpServers` and `skills` in `SettingsSnapshot.sections`.

That is a Backend/Frontend contract mismatch: the Backend claims sections are available before it has renderable section data.

## Next

Change the current App Server Settings snapshot to advertise only renderable sections: `agents` and `commonSettings`. Keep the protocol enum values for future MCP/Skills work.
