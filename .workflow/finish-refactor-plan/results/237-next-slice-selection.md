# Next Slice Selection

Selected the App Shell Contracts webview type split as the next refactor slice.

Why this slice:
- `packages/app-shell-contracts/src/webviewTypes.ts` is now the largest
  app-shell-contracts source file at 291 lines.
- It mixes Webview/App Shell message envelopes, bootstrap metadata,
  diagnostics, settings records, preferences, telemetry, and runtime error
  payload types in one module.
- The file is on the App Shell/Frontend seam, so preserving its public export
  surface while improving internal ownership reduces risk before adding more
  shell behavior.

Scope:
- Split `webviewTypes.ts` into focused internal type modules.
- Keep package-level exports compatible through `src/index.ts`.
- Keep `webviewTypes.ts` as the compatibility facade for existing imports.
- Do not change message names, payload fields, union members, optionality,
  runtime behavior, generated protocol bindings, or Frontend state logic.

Primary risks:
- Breaking existing imports from `@openaide/app-shell-contracts` or
  `./webviewTypes.js`.
- Creating confusing dependencies between message envelopes and settings types.
- Accidentally changing Webview message union member shapes while moving types.
- Editing generated `dist/` output by hand.

