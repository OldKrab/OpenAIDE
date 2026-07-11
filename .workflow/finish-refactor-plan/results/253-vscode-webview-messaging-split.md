# VS Code Webview Messaging Split

## Contract

Split `apps/vscode-extension/src/webview/messaging.ts` into focused App Shell
message-routing modules while preserving `handleWebviewMessage` as the stable facade.

Ownership:

- `messaging.ts`: message validation, lifecycle logging, top-level route order, and
  runtime-error response tagging.
- `messagingContext.ts`: host message context shape.
- `messagingFields.ts`: safe log/telemetry field extraction and generic message field
  helpers only.
- `messagingSettings.ts`: diagnostics, settings, preferences, custom Agent, and Agent
  enablement routes.
- `messagingAgents.ts`: Agent config option, authentication, session list, and config
  option mutation routes.
- `messagingShell.ts`: surface commands and VS Code shell capabilities.
- `messagingTasks.ts`: Task, chat, tool detail, prompt, cancel, archive/restore, and
  permission-response routes.

Do not change App Server Protocol, runtime RPC semantics, shared Frontend behavior, VS
Code command registration, route order, postback payloads, logging redaction,
workspace-root fallback, custom Agent side effects, or workspace path validation.

Focused tests:

- Existing `apps/vscode-extension/src/webview/messaging.test.ts` remains the behavior
  suite for moved routes.
- `npm run check` covers TypeScript import and union-shape safety.

## Implementation

Implemented the split by moving route groups and helper types into focused modules. The
public import surface remains `handleWebviewMessage` from `messaging.ts`.

Production source sizes after split:

- `messaging.ts`: 68 lines.
- `messagingAgents.ts`: 87 lines.
- `messagingContext.ts`: 32 lines.
- `messagingFields.ts`: 68 lines.
- `messagingSettings.ts`: 118 lines.
- `messagingShell.ts`: 58 lines.
- `messagingTasks.ts`: 141 lines.

## Review

`$doomsday-review` round 1:

- Correctness: no findings.
- Requirements/tests: accepted one Medium missing-test finding for moved public routes.
- Code quality: accepted one Low boundary finding for Agent route defaults living in
  `messagingFields.ts`.

Fixes:

- Added direct route coverage for runtime health, diagnostics snapshot/export, Agent
  enablement, direct task list/snapshot, tool detail, mark-read, cancel, and permission
  response.
- Moved Agent route defaults and empty response helpers into `messagingAgents.ts`, leaving
  `messagingFields.ts` as generic field/logging helpers.

Round 2 narrow fix review: no findings.

## Verification

Focused checks already run:

- `npm --workspace openaide-vscode-extension test -- src/webview/messaging.test.ts`: pass.
- `npm run check`: pass.

Final checks:

- `npm test`: pass.
- `git diff --check`: pass.
- `jq empty .workflow/finish-refactor-plan/state.json`: pass.
- Changed production source-size scan: largest split file is `messagingTasks.ts` at 141
  lines.

## Commit

This commit: `refactor: split vscode webview messaging routes`.

## Next

After this slice is committed, select the next compact refactor slice from the current
plan and file-size/boundary pressure.
