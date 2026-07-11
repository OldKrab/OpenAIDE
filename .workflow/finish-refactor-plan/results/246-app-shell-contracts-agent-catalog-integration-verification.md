# App Shell Contracts Agent Catalog Integration Verification

The App Shell Contracts Agent Catalog split passed integration verification.

Checks:
- `npm run check --workspace @openaide/app-shell-contracts`
- `npm run build --workspace @openaide/app-shell-contracts`
- `npm run check`
- `npm test -- --runInBand`
- `git diff --check`
- `jq empty .workflow/finish-refactor-plan/state.json`
- Exported Agent Catalog type/value-name compatibility diff against the
  planning commit.
- Source-size scan for changed app-shell-contracts source files.
- Value-level smoke check for built-ins, custom parsing/filtering, runtime
  projection, display labels, and icon ids from the generated package build.

Notes:
- `agentCatalog.ts` remains a compatibility facade over focused modules in
  `src/agentCatalog/`.
- The exported Agent Catalog type/value-name diff against the planning commit
  is empty.
- Changed app-shell-contracts source files remain below the 400-line production
  source limit. The largest focused Agent Catalog module is
  `agentCatalog/settings.ts` at 77 lines.

