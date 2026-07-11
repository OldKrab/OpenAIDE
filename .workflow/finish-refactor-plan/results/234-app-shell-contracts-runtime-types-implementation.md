# App Shell Contracts Runtime Types Implementation

Implemented the accepted App Shell Contracts runtime type split only.

Changed modules:
- `runtimeTypes.ts` is now a compatibility facade that re-exports focused
  runtime type modules.
- `runtime/primitives.ts` owns shared literal and scalar-ish common types.
- `runtime/chat.ts` owns normalized chat, activity, permission, attachment,
  and message page types.
- `runtime/task.ts` owns task summaries, snapshots, and task list results.
- `runtime/agent.ts` owns agent setup, config options, auth, probe, listed
  session, and custom-agent types.
- `runtime/requests.ts` owns request parameter types and
  `RuntimeRequestParamsByMethod`.
- `runtime/system.ts` owns runtime diagnostics, settings, health, empty
  result, and `RuntimeResultByMethod`.

Focused verification before review:
- `npm run check --workspace @openaide/app-shell-contracts`
- `npm run build --workspace @openaide/app-shell-contracts`
- `npm run check`
- Exported runtime type-name compatibility diff against the planning commit.
- Source-size scan for changed app-shell-contracts source files.

