# 299 App Server Agent Settings Details

## Scope

Implemented the A6 read side for App Server-owned Agent Settings details.

## Decisions

- Added typed App Server Protocol method `settings/getAgentDetails`.
- Kept the method read-only: it returns details without publishing Agent collection events.
- Projected details from known built-ins plus the persisted Agent catalog overlay, because disabled Agents do not appear in the summary Agent collection snapshot.
- Persisted custom Agent `icon` and exact original `commandLine` in the App Server catalog so custom Settings rows can reload without shell-local UI data or lossy command-line reconstruction.
- Frontend Settings refresh now prefers the typed Backend details read and falls back to legacy `settings.snapshot` only when typed Backend requests are unavailable.
- Frontend maps App Server Protocol detail rows into Agent-only Settings state, preserving current UI components while avoiding stale overwrite of non-Agent Settings sections during the transition.

## Remaining Gap

Launch-affecting custom Agent edits still need a separate App Server workflow with explicit warning, new Agent identity creation, and old-identity local cleanup. Full App Server ownership of non-Agent Settings sections remains future A6/A8 work.

## Verification

- `$doomsday-review` with independent correctness, requirements, and code-quality review passes; fixed stale UI reconciliation, command-line round-trip, status encapsulation, and Backend metadata ownership findings.
- `cargo fmt --all --check`
- `cargo test -p openaide-runtime`
- `cargo test -p openaide-app-server-protocol`
- `npm run protocol:check`
- `npm run build --workspace @openaide/app-server-client`
- `npm run check --workspace openaide-frontend`
- `npm run test --workspace openaide-frontend`
- `npm run check --workspace openaide-vscode-extension`
- `npm test --workspace openaide-vscode-extension -- src/webview/messaging.test.ts`
- `npm run test --workspace @openaide/app-server-client`
- `git diff --check`
- `jq empty .workflow/finish-refactor-plan/state.json`
- Source-size scan: changed production files remain below the 400-line limit; largest scanned production file is 300 lines.

## Next

Design and implement the launch-affecting Custom Agent replacement workflow.
