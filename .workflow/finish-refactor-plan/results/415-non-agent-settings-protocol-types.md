# P415 non-Agent Settings protocol types

## Result

Added protocol source and generated TypeScript bindings for MCP and Skills Settings read methods.

## Implementation

- Added typed methods:
  - `settings/getMcpServers`
  - `settings/getSkills`
- Added safe protocol records:
  - `SettingsMcpServersParams`
  - `SettingsMcpServersResult`
  - `SettingsMcpServerRecord`
  - `SettingsSkillsParams`
  - `SettingsSkillsResult`
  - `SettingsSkillRecord`
  - shared safe scope, notice, transport, and status enums.
- Updated Rust protocol method constants and `ProtocolMethod` structs.
- Updated TypeScript generation declarations, method constants, method maps, response aliases, and generated binding tests.
- Regenerated `packages/app-server-client/src/generated/protocol.ts`.

## Verification

- `cargo fmt --all --check`
- `cargo check -p openaide-app-server-protocol`
- `cargo test -p openaide-app-server-protocol generated_bindings_include_protocol_method_maps`
- `npm run protocol:generate`
- `npm run protocol:check`
- `npm run build --workspace @openaide/app-server-client`

## Next

Wire App Server runtime handlers and projection sources. The new methods are typed but not yet routed by `RpcGateway`.
