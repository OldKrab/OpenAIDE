# P327 VS Code Reveal File Resolve/Open

## Scope

Wired VS Code `shell/revealFile` handling through a shell-private runtime
resolve method so registered App Server file handles can open real files without
raw paths crossing Frontend.

## Decisions

- `shell.fileReveal.resolve` is a legacy host/private runtime method, not a
  generated App Server Protocol method.
- Raw resolved paths are returned only to trusted VS Code host code.
- The webview bridge continues to carry only `fileHandleId` and safe label.
- VS Code validates the resolved path with the existing workspace boundary
  helper before opening the document.
- Unresolved handles return `{ revealed: false }`.

## Verification

- `cargo fmt --all --check`
- `cargo test -p openaide-runtime shell_file_reveal_resolve_returns_registered_target -- --nocapture`
- `npm run check --workspace openaide-vscode-extension`
- `npm run test --workspace openaide-vscode-extension -- messaging.test.ts`

## Next

Move to the next top-level backlog slice after A4: App Server-owned Projects,
Settings, Agent identity, and core product modules.
