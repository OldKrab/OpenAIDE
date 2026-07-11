# Next Slice Selection

Selected the App Shell Contracts runtime type split as the next refactor slice.

Why this slice:
- `packages/app-shell-contracts/src/runtimeTypes.ts` is 399 lines, directly at
  the production source-size limit.
- The file mixes request maps, task/chat render model types, agent setup types,
  runtime diagnostics/settings types, and shared primitives in one module.
- App Shell contracts sit on the Backend/Frontend seam, so keeping the public
  import surface stable while improving internal ownership is a good next
  step before adding more shell behavior.

Scope:
- Split `runtimeTypes.ts` into focused internal type modules.
- Keep package-level exports compatible through `src/index.ts`.
- Keep `runtimeTypes.ts` as the compatibility facade for existing imports.
- Do not change runtime behavior, message names, field names, generated
  protocol bindings, or Frontend state logic in this slice.

Primary risks:
- Circular type imports between request maps and domain model types.
- Breaking existing imports from `@openaide/app-shell-contracts`.
- Accidentally editing generated `dist/` files by hand instead of using the
  package build.

