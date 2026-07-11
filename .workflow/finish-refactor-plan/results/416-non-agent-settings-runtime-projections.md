# P416 - Non-Agent Settings Runtime Projections

## Result

Implemented App Server runtime support for the non-Agent Settings projection methods introduced in P415.

## Changes

- Added App Server-owned MCP server and Skills settings workflow traits and initial runtime services.
- Routed `settings/getMcpServers` and `settings/getSkills` through `RpcGateway`.
- Wired the runtime factory to provide the new settings workflows.
- Kept default test snapshots conservative, and advertised MCP/Skills sections only through the backend-settings catalog path used by the real App Server factory.
- Added focused tests for non-Agent Settings protocol reads and Settings section availability.

## Review

Local doomsday-style review found one design issue before commit: `SettingsCatalog::default()` briefly advertised MCP/Skills without runtime projection source wiring. Fixed by keeping defaults conservative and advertising those sections only through `SettingsCatalog::with_backend_settings`.

Findings after fix: none.

## Verification

- `cargo fmt --all --check`
- `cargo check -p openaide-runtime`
- `cargo test -p openaide-runtime settings::tests`
- `cargo test -p openaide-runtime protocol_edge::tests::non_agent_settings_reads_return_app_server_owned_projections`
- `git diff --check`
- Rust production source-size guard, excluding tests and generated files

## Next

P417 should wire Frontend Settings state and intent loading for the App Server-owned MCP/Skills projection methods, restoring the UI tabs with explicit loading, empty, and error states.
