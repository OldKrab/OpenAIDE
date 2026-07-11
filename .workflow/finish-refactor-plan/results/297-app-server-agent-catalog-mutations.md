# 297 App Server Agent Catalog Mutations

## Scope

- Added typed App Server Protocol methods:
  - `agent/saveCustom`
  - `agent/deleteCustom`
  - `agent/setEnabled`
- Added generated TypeScript protocol bindings for the new methods.
- Added a shared live `AgentRegistryHandle` for App Server runtime paths.
- Wired App Server Agent mutations through `AgentProductApi`, `AgentCatalogStore`, and `RpcGateway`.

## Decisions

- Catalog mutations are Backend product operations, not shell settings writes.
- Successful mutations write `agents/catalog.json`, replace the live registry handle, return a renderable `AgentCollectionSnapshot`, and publish `AgentCollectionUpdated`.
- Secret env values are not stored in the catalog. The catalog stores only secret env names; launch-time value resolution remains a Backend-initiated shell request.
- Existing initialized clients observe mutation results through the same event stream used by Agent probe status changes.

## Verification

- `cargo test -p openaide-runtime agent_ -- --nocapture`
- `cargo test -p openaide-runtime -- --test-threads=1 --nocapture`
- `cargo test -p openaide-app-server-protocol -- --nocapture`
- `npm run protocol:generate`
- `npm run protocol:check`
- `npm run build --workspace @openaide/app-server-client`
- `npm run test --workspace @openaide/app-server-client`

## Next

- Route Frontend and VS Code settings actions through the typed App Server mutation methods with responsive pending/error presentation.
