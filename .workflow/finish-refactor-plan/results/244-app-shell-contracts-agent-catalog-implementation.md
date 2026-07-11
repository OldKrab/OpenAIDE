# App Shell Contracts Agent Catalog Implementation

Implemented the accepted App Shell Contracts Agent Catalog split only.

Changed modules:
- `agentCatalog.ts` is now a compatibility facade that re-exports focused
  Agent Catalog modules.
- `agentCatalog/icons.ts` owns icon ids, the icon list, and icon
  normalization.
- `agentCatalog/types.ts` owns catalog entry, custom settings, and runtime
  projection record types.
- `agentCatalog/builtins.ts` owns built-in definitions, default agent, and
  built-in lookup helpers.
- `agentCatalog/settings.ts` owns custom-agent settings parsing, built-in
  override merging, and local normalization helpers.
- `agentCatalog/runtime.ts` owns runtime catalog projection.
- `agentCatalog/display.ts` owns display label fallback.

Focused verification before review:
- `npm run check --workspace @openaide/app-shell-contracts`
- `npm run build --workspace @openaide/app-shell-contracts`
- `npm run check`
- Exported Agent Catalog type/value-name compatibility diff against the
  planning commit.
- Source-size scan for changed app-shell-contracts source files.
- Value-level smoke check for built-ins, custom parsing/filtering, runtime
  projection, display labels, and icon ids from the generated package build.

