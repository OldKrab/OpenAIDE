# App Shell Contracts Runtime Types API Contract

Accept the App Shell Contracts runtime type split.

Public API:
- Keep `@openaide/app-shell-contracts` exporting the same runtime type names
  from the package root.
- Keep existing imports from `./runtimeTypes.js` working for `webviewTypes.ts`
  and any external package consumer.
- Keep all exported type names, string literal method names, union members,
  field names, optionality, and structural shapes unchanged.
- Keep `RuntimeRequestParamsByMethod` and `RuntimeResultByMethod` as the typed
  request/result maps used by App Shell and Frontend code.

Internal module contract:
- Convert `runtimeTypes.ts` into a facade that re-exports focused type modules.
- Create focused modules under `packages/app-shell-contracts/src/runtime/`:
  - `primitives.ts` for shared literals and scalar-ish common types.
  - `requests.ts` for request parameter types and
    `RuntimeRequestParamsByMethod`.
  - `chat.ts` for normalized chat, activity, permission, attachment, and page
    types.
  - `task.ts` for task summaries, snapshots, and task list results.
  - `agent.ts` for config options, auth, probe, custom-agent, listed-session,
    and agent session types.
  - `system.ts` for runtime settings, diagnostics, health, empty result, and
    `RuntimeResultByMethod`.
- Keep type dependencies one directional enough to avoid cycles:
  primitives -> no runtime imports; chat -> primitives; task -> primitives and
  chat; agent -> primitives; requests -> primitives, chat, and agent; system ->
  task, chat, and agent.
- Do not introduce runtime values, helper functions, validation logic, or
  behavior in this slice; this is a type-layout split only.

Behavior to preserve:
- TypeScript consumers can compile without changing imports.
- `webviewTypes.ts` continues to import the same runtime type names.
- Package build still emits declarations for the facade and focused modules.

Verification:
- `npm run check --workspace @openaide/app-shell-contracts`
- `npm run build --workspace @openaide/app-shell-contracts`
- `npm run check`
- `npm test -- --runInBand`
- `git diff --check`
- Source-size scan for changed production app-shell-contracts files.

