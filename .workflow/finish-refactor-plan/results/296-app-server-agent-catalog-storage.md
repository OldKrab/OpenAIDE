# 296 App Server Agent Catalog Storage

## Scope

- Added App Server-owned Agent catalog storage under the protected state root.
- Loaded runtime AgentRegistry from the opened Store in both Runtime and stdio protocol edge startup paths.
- Removed the VS Code runtime Agent catalog environment-variable bridge.
- Disabled the legacy VS Code shell-owned custom Agent mutation path so it cannot write a catalog ignored by Backend.

## Decisions

- `agents/catalog.json` is an App Server storage file with explicit schema version.
- Missing catalog means default built-in Agents are available.
- Stored records are an overlay over default built-ins: disabled records remove built-ins, custom records add or replace custom launch definitions.
- Secret values are not persisted in the catalog; stored records keep only secret env names. Values still flow through the existing shell host bridge at Agent launch time.

## Verification

- `cargo test -p openaide-runtime -- --test-threads=1 --nocapture`
- `npm run build --workspace openaide-vscode-extension`
- `npm test --workspace openaide-vscode-extension -- src/settings/agents.test.ts src/webview/messaging.test.ts`
- `cargo test -p openaide-runtime initialize_uses_stored_agent_catalog_in_production_startup_path -- --nocapture`

## Next

- Add typed custom Agent mutation APIs that write this catalog and publish Agent collection updates.
