# App Shell Contracts Webview Types Integration Verification

The App Shell Contracts webview type split passed integration verification.

Checks:
- `npm run check --workspace @openaide/app-shell-contracts`
- `npm run build --workspace @openaide/app-shell-contracts`
- `npm run check`
- `npm test -- --runInBand`
- `git diff --check`
- `jq empty .workflow/finish-refactor-plan/state.json`
- Exported webview type-name compatibility diff against the planning commit.
- Source-size scan for changed app-shell-contracts source files.

Notes:
- `webviewTypes.ts` remains a compatibility facade over focused modules in
  `src/webview/`.
- The exported webview type-name diff against the planning commit is empty.
- Changed app-shell-contracts source files remain below the 400-line production
  source limit. The largest focused webview module is `webview/messages.ts` at
  123 lines.

