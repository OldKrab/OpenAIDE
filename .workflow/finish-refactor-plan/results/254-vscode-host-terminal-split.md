# VS Code Host Terminal Split

## Contract

Split `apps/vscode-extension/src/runtime/hostTerminal.ts` into focused VS Code App Shell
runtime modules while preserving `registerTerminalHostHandlers` and
`TerminalHostManager` as the stable public surface.

Ownership:

- `hostTerminal.ts`: runtime host request registration and terminal lifecycle manager.
- `hostTerminalTypes.ts`: terminal request/record/result types and host method names.
- `hostTerminalParams.ts`: request parameter validation and workspace `cwd`
  canonicalization.
- `hostTerminalEnvironment.ts`: terminal environment construction, VS Code integrated
  terminal env expansion, PATH key handling, and Codex bundled tool path discovery.
- `hostTerminalOutput.ts`: stdout/stderr decoder flushing, UTF-8-safe output append, and
  byte-limit truncation.

Do not change runtime RPC semantics, workspace boundary policy, terminal lifecycle
policy, spawn options, kill/release timing, exit waiter behavior, env expansion, PATH
repair, output byte limiting, UTF-8 decoder behavior, or tests.

Focused tests:

- Existing `apps/vscode-extension/src/runtime/hostTerminal.test.ts` remains the behavior
  suite for moved terminal runtime behavior.
- `npm run check` covers TypeScript import and type-boundary safety.

## Implementation

Implemented the split by moving request validation, environment construction, shared
types, and output handling into focused modules. `hostTerminal.ts` remains the only
public manager/registration entry point.

Production source sizes after split:

- `hostTerminal.ts`: 175 lines.
- `hostTerminalEnvironment.ts`: 115 lines.
- `hostTerminalOutput.ts`: 33 lines.
- `hostTerminalParams.ts`: 91 lines.
- `hostTerminalTypes.ts`: 47 lines.

## Review

`$doomsday-review`:

- Correctness: no findings.
- Requirements/tests: no findings.
- Code quality: local pass found no findings.

## Verification

Focused checks already run:

- `npm --workspace openaide-vscode-extension test -- src/runtime/hostTerminal.test.ts`:
  pass.
- `npm run check`: pass.

Final checks:

- `npm test`: pass.
- `git diff --check`: pass.
- `jq empty .workflow/finish-refactor-plan/state.json`: pass.
- Changed production source-size scan: largest split file is `hostTerminal.ts` at 175
  lines.

## Commit

This commit: `refactor: split vscode host terminal runtime`.

## Next

After this slice is committed, select the next compact refactor slice from the current
plan and file-size/boundary pressure.
