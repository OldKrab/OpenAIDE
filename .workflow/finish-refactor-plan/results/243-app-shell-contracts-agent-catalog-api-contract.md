# App Shell Contracts Agent Catalog API Contract

Accept the App Shell Contracts Agent Catalog split.

Public API:
- Keep `@openaide/app-shell-contracts` exporting the same Agent Catalog type
  and value names from the package root.
- Keep existing imports from `./agentCatalog.js` working for package consumers
  and sibling contract modules.
- Keep all exported type names, value names, literal values, field names,
  optionality, and structural shapes unchanged.
- Keep `builtInAgents`, `defaultAgent`, `agentCatalogEntry`,
  `resolveAgentCatalogEntry`, `agentDisplayLabel`, `normalizedAgentIcon`,
  `customAgentsFromSettings`, `agentCatalogFromSettings`, and
  `runtimeAgentCatalog` behavior unchanged.

Internal module contract:
- Convert `agentCatalog.ts` into a facade that re-exports focused modules.
- Create focused modules under `packages/app-shell-contracts/src/agentCatalog/`:
  - `icons.ts` for `AgentIconId`, `agentIconIds`, and
    `normalizedAgentIcon`.
  - `types.ts` for `AgentCatalogEntry`, `CustomAgentSettingsRecord`, and
    `RuntimeAgentCatalogRecord`.
  - `builtins.ts` for `builtInAgents`, `defaultAgent`,
    `agentCatalogEntry`, and `resolveAgentCatalogEntry`.
  - `settings.ts` for custom-agent parsing, settings catalog merging, string
    and environment normalization helpers.
  - `runtime.ts` for `runtimeAgentCatalog`.
  - `display.ts` for `agentDisplayLabel`.
- Keep dependency direction simple: types imports icon types; builtins imports
  types; settings imports icons, types, and builtins; runtime imports types;
  display imports builtins.
- Do not introduce runtime validation libraries, new product behavior, or
  generated files in this slice; this is a module-layout split only.

Behavior to preserve:
- Frontend `AgentIcon`, Settings, and composer option code compile without
  changing imports.
- Built-in Codex and OpenCode records remain byte-for-byte equivalent in
  product shape.
- Custom-agent settings parsing continues to drop invalid records, default
  missing labels/descriptions/icons as before, filter invalid secret env names,
  apply built-in enabled overrides, and omit custom records whose ids collide
  with built-ins.
- Runtime catalog projection continues to include only enabled agents and the
  existing runtime-safe fields.

Verification:
- `npm run check --workspace @openaide/app-shell-contracts`
- `npm run build --workspace @openaide/app-shell-contracts`
- `npm run check`
- `npm test -- --runInBand`
- `git diff --check`
- Exported Agent Catalog type/value-name compatibility diff against the
  planning commit.
- Source-size scan for changed app-shell-contracts files.

