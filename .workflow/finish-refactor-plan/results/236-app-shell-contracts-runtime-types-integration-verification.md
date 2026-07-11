# App Shell Contracts Runtime Types Integration Verification

The App Shell Contracts runtime type split passed integration verification.

Checks:
- `npm run check --workspace @openaide/app-shell-contracts`
- `npm run build --workspace @openaide/app-shell-contracts`
- `npm run check`
- `npm test -- --runInBand`
- `git diff --check`
- `jq empty .workflow/finish-refactor-plan/state.json`
- Exported runtime type-name compatibility diff against the planning commit.
- Source-size scan for changed app-shell-contracts source files.

Notes:
- `runtimeTypes.ts` remains a compatibility facade over focused modules in
  `src/runtime/`.
- The exported runtime type-name diff against the planning commit is empty.
- Changed app-shell-contracts source files remain below the 400-line production
  source limit: the largest new focused module is `runtime/agent.ts` at 118
  lines, and `runtimeTypes.ts` is 6 lines.

