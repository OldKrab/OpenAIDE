# App Shell Contracts Webview Types Implementation

Implemented the accepted App Shell Contracts webview type split only.

Changed modules:
- `webviewTypes.ts` is now a compatibility facade that re-exports focused
  webview type modules.
- `webview/notifications.ts` owns runtime notification types.
- `webview/hostRequest.ts` owns the generic host JSON-RPC request envelope.
- `webview/preferences.ts` owns settings-tab, settings-scope, composer
  shortcut, and preferences records.
- `webview/bootstrap.ts` owns surface, bootstrap, and webview agent option
  types.
- `webview/requestMeta.ts` owns snapshot, options, and session-list metadata
  mixins.
- `webview/telemetry.ts` owns the telemetry payload type.
- `webview/settings.ts` owns diagnostics and settings record types.
- `webview/messages.ts` owns Webview/App Shell message unions and related
  message payload aliases.

Focused verification before review:
- `npm run check --workspace @openaide/app-shell-contracts`
- `npm run build --workspace @openaide/app-shell-contracts`
- `npm run check`
- Exported webview type-name compatibility diff against the planning commit.
- Source-size scan for changed app-shell-contracts source files.

