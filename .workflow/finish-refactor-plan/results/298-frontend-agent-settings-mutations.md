# 298 Frontend Agent Settings Mutations

## Scope

Implemented the Frontend/VS Code bridge slice for App Server-owned custom Agent mutations.

## Decisions

- Shared Frontend Settings callbacks now prefer typed `BackendConnection.request` calls for `agent/saveCustom`, `agent/deleteCustom`, and `agent/setEnabled`.
- Existing custom Agent saves pass the custom `agentId` through to App Server and replace that catalog record under the same identity for the current metadata-edit path; new custom Agent saves omit `agentId`.
- Legacy `agent.custom.*` and `agent.enabled.set` host messages remain only as fallback when a shell does not expose typed Backend requests.
- Mutation success updates the current client immediately:
  - Settings row save/delete/update acknowledgement state is reconciled.
  - New Task Agent options are refreshed from the returned App Server Agent collection.
  - Default Agent selection is repaired when the selected Agent disappears.
- Typed request errors are rendered through `settings:error` and are not replayed through the legacy shell-owned mutation path.

## Remaining Gap

App Server Agent collection snapshots are summary-only. Full reloadable Settings details for custom Agents, including command line, icon, and env rows, still require an App Server-owned Settings/Agent details read API. Until that exists, the legacy VS Code settings snapshot still carries some UI-only Settings details.

Launch-affecting custom Agent edits still need a separate App Server workflow with an explicit warning, new Agent identity creation, and old-identity local cleanup. The generic save path must not accidentally create duplicate visible Agents.

## Verification

- `npm run test --workspace openaide-frontend -- src/components/appControllerCallbacks.test.ts`
- `npm run test --workspace openaide-frontend -- src/components/appControllerCallbacks.test.ts src/state/appReducer.test.ts`
- `npm run check --workspace openaide-frontend`
- `npm run test --workspace openaide-frontend`
- `cargo test -p openaide-runtime agent::catalog_store::tests`
- `cargo test -p openaide-runtime`
- `npm run check --workspace openaide-vscode-extension`
- `npm test --workspace openaide-vscode-extension -- src/webview/messaging.test.ts`

## Next

Design and implement the App Server-owned Settings/Agent details read API so Settings can reload custom Agent details from Backend state and stop depending on the legacy VS Code settings snapshot for Agent product data.
