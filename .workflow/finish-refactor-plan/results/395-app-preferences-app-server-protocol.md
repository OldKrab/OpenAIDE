# P395 App Preferences App Server Protocol

## Result

- Added typed `settings/getPreferences` and `settings/updatePreferences` App Server Protocol methods.
- Added App Server durable app preference storage under the Backend settings storage area.
- Included app preferences in initialize settings snapshots so reload state comes from App Server.
- Moved Frontend composer submit shortcut mutation to `BackendConnection.request`.
- Preserved responsive UI with immediate local preference presentation and App Server result reconciliation.
- Removed VS Code/globalState preference persistence, shell preference result messages, standalone dev-host preference routing, and obsolete tests.

## Verification

- `cargo fmt --all --check`
- `cargo test -p openaide-runtime settings::tests::`
- `cargo test -p openaide-runtime storage::app_preferences::tests::`
- `cargo test -p openaide-runtime protocol_edge::tests::app_preferences_get_and_update_use_app_server_protocol`
- `cargo test -p openaide-app-server-protocol generated_bindings_include_protocol_method_maps`
- `npm run protocol:generate`
- `npm run protocol:check`
- `npm run build --workspace @openaide/app-server-client`
- `npm run check --workspace @openaide/app-shell-contracts`
- `npm run check --workspace openaide-frontend`
- `npm run check --workspace openaide-vscode-extension`
- `npm run test --workspace openaide-frontend -- appControllerCallbacks.test.ts appServerInitialSnapshot.test.ts appController.test.tsx devHost.test.ts hostMessageRouter.test.ts`
- `npm run test --workspace openaide-vscode-extension -- messaging.test.ts surfaces.test.ts`
