# Next Slice Selection

Selected the App Shell Contracts Agent Catalog split as the next refactor
slice.

Why this slice:
- `packages/app-shell-contracts/src/agentCatalog.ts` is the remaining
  app-shell-contracts source file that still mixes several ownership concerns.
- The module currently owns icon vocabulary, built-in agent definitions,
  custom-agent settings normalization, built-in override merging, runtime
  catalog projection, and display fallback helpers.
- App Shell contracts sit on the shell/Frontend seam, so preserving the public
  import surface while separating catalog responsibilities keeps future shell
  work easier to reason about.

Scope:
- Split `agentCatalog.ts` into focused internal modules.
- Keep package-level exports compatible through `src/index.ts`.
- Keep existing imports from `./agentCatalog.js` working for package consumers
  and sibling contract modules.
- Do not change built-in agent ids, labels, descriptions, command lines,
  default agent selection, custom-agent normalization, icon list, runtime
  projection shape, or display-label fallback behavior.

Primary risks:
- Breaking existing imports of `AgentIconId`, `agentIconIds`, `builtInAgents`,
  `defaultAgent`, `agentCatalogEntry`, `resolveAgentCatalogEntry`,
  `agentDisplayLabel`, `normalizedAgentIcon`,
  `agentCatalogFromSettings`, `customAgentsFromSettings`, or
  `runtimeAgentCatalog`.
- Creating circular imports between icon, built-in, and settings modules.
- Accidentally changing custom-agent validation or built-in override behavior
  while moving helpers.
- Editing generated `dist/` output by hand.

