# P396 Remaining Shell Contract Audit

## Result

- Confirmed the old composer preference shell messages are removed.
- Remaining shell messages are shell/bootstrap capabilities:
  - telemetry
  - App Server backend-initiated shell request forwarding
  - diagnostics
  - workspace roots bootstrap
  - developer settings unlock
  - surface navigation
  - local path open/reveal
- Found one transitional product-state gap: `context.pickFile`/`context.file.result` still returns the old Frontend attachment shape for shell-native file selection.

## Next Packet

P397 should remove or reroute shell-native file picking so composer attachments are App Server-owned handles. The simplest v1-compatible option is likely to remove the shell picker entry point from product UI and rely on the App Server-backed file browser first; a later shell-native picker can hand raw paths directly to App Server attachment methods without storing raw paths in Frontend state.

## Verification

- `rg -n "app\\.composerSubmitShortcut|app\\.preferences|collectAppPreferences|AppPreferencesStore|broadcastAppPreferences" apps packages openaide-rs docs .workflow -g '!packages/app-server-client/src/generated/protocol.ts'`
- Webview shell contract read from `packages/app-shell-contracts/src/webview/messages.ts`
- Usage scan for `context.pickFile`, `context.file.result`, `tool.openPath`, diagnostics, developer unlock, and workspace roots.
