# P408 advertise only renderable Settings sections

## Result

Aligned App Server Settings snapshot section availability with the current renderable Settings UI.

## Implementation

- `SettingsCatalog::product_defaults` now advertises only `agents` and `commonSettings`.
- Requests for unimplemented sections such as `mcpServers` return an empty section list instead of claiming availability.
- Snapshot tests now assert the renderable section list.
- Protocol enum values for MCP/Skills remain for future App Server-owned projections.

## Verification

- `cargo fmt --all --check`
- `cargo check -p openaide-runtime -p openaide-app-server-protocol`
- `cargo test -p openaide-runtime settings::tests`
- `cargo test -p openaide-runtime snapshots::tests`
