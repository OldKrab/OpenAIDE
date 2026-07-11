# VS Code Skill Settings Split

## Contract

Split focused skill settings helpers out of
`apps/vscode-extension/src/settings/skills.ts` while preserving `scanSkills`,
`parseSkillMetadata`, and `SkillScanBase` as the stable settings API.

Ownership:

- `skills.ts`: public scan facade, scan-limit policy, base composition, and
  public re-exports.
- `skillDiscovery.ts`: workspace/global skill base construction, filesystem
  directory discovery, nested dot-directory discovery, and unreadable root
  issue creation.
- `skillMetadata.ts`: SKILL.md metadata block parsing, fallback metadata
  parsing, scalar cleanup, and inline tag-list parsing.
- `skillRecords.ts`: individual skill record scanning, scan issue records,
  shadowing, and Node filesystem error narrowing.
- `skillTypes.ts`: local skill scanner helper types.

Do not change workspace/global base ordering, nested dot-directory discovery,
missing/unreadable root handling, warning sanitization, scan limit behavior,
metadata parsing fallback, shadowing priority, public exports, settings
snapshot contracts, runtime RPC behavior, App Server Protocol records, or
product UX text in this slice.

Focused tests:

- Existing `apps/vscode-extension/src/settings/skills.test.ts` remains the
  behavior regression suite.
- VS Code Extension test/check commands cover moved type boundaries.

## Implementation

Implemented the split by keeping `skills.ts` as the public facade and moving
discovery, metadata parsing, record assembly, and local helper types into
focused settings modules.

Also fixed an existing VS Code App Shell type-narrowing issue in
`messagingTasks.ts` that surfaced during the extension check: the task-list
refresh helper now accepts the precise archived-list payload shape used by
`task.list`, `task.archive`, and `task.restore`.

Production source sizes after split:

- `skills.ts`: 39 lines.
- `skillDiscovery.ts`: 83 lines.
- `skillMetadata.ts`: 48 lines.
- `skillRecords.ts`: 112 lines.
- `skillTypes.ts`: 24 lines.
- `messagingTasks.ts`: 145 lines.

## Review

`$doomsday-review`:

- Correctness/spec/tests: no findings.
- Code quality: local pass found no findings.

## Verification

Focused checks:

- `npm --workspace openaide-vscode-extension test -- src/settings/skills.test.ts src/webview/messaging.test.ts`: pass.
- `npm run check --workspace openaide-vscode-extension`: pass.

Final checks:

- `npm run check`: pass.
- `npm test`: pass.
- `git diff --check`: pass.
- `jq empty .workflow/finish-refactor-plan/state.json`: pass.
- Changed production source-size scan: largest changed production file is
  `messagingTasks.ts` at 145 lines; largest new skill settings module is
  `skillRecords.ts` at 112 lines.

## Commit

This commit: `refactor: split vscode skill settings scan`.

## Next

After this slice is committed, select the next compact refactor slice from the
current plan and architecture/file-size pressure.
