# App Shell Contracts Webview Types API Contract

Accept the App Shell Contracts webview type split.

Public API:
- Keep `@openaide/app-shell-contracts` exporting the same webview type names
  from the package root.
- Keep existing imports from `./webviewTypes.js` working for package consumers.
- Keep all exported type names, message `type` strings, union members, field
  names, optionality, and structural shapes unchanged.
- Keep `WebviewToHostMessage` and `HostToWebviewMessage` as the typed message
  unions used by Frontend and shell code.

Internal module contract:
- Convert `webviewTypes.ts` into a facade that re-exports focused type modules.
- Create focused modules under `packages/app-shell-contracts/src/webview/`:
  - `notifications.ts` for runtime notification and host request types.
  - `preferences.ts` for settings-tab, settings-scope, preferences, surface,
    bootstrap, snapshot/options/session-list metadata, and telemetry types.
  - `settings.ts` for diagnostics snapshot and settings record types.
  - `messages.ts` for `RuntimeErrorPayload`, `WebviewTaskListMessage`,
    `WebviewToHostMessage`, `HostToWebviewMessage`, and
    `RuntimeSettingsPatch`.
- Keep type dependencies simple: settings and preferences may import runtime
  and agent catalog types; messages may import settings, preferences, runtime,
  and agent catalog types; notifications imports only runtime catalog/result
  types; the facade imports nothing and only re-exports.
- Do not introduce runtime values, helper functions, validation logic, or
  behavior in this slice; this is a type-layout split only.

Behavior to preserve:
- TypeScript consumers compile without changing imports.
- Frontend host bridge, dev host, reducers, and message router continue to use
  the same message union types.
- Package build still emits declarations for the facade and focused modules.

Verification:
- `npm run check --workspace @openaide/app-shell-contracts`
- `npm run build --workspace @openaide/app-shell-contracts`
- `npm run check`
- `npm test -- --runInBand`
- `git diff --check`
- Exported webview type-name compatibility diff against the planning commit.
- Source-size scan for changed app-shell-contracts files.

