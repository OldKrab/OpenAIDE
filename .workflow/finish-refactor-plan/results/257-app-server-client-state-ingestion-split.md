# App Server Client State Ingestion Split

## Contract

Split pure snapshot update helpers out of `packages/app-server-client/src/stateIngestion.ts`
while preserving `createSubscriptionIngestionState` and `applySubscriptionEvent` as the
stable public API.

Ownership:

- `stateIngestion.ts`: public state shape, public apply API, subscription scope matching,
  cursor validation, resync classification, ignored/applied result construction, and state
  cursor advancement.
- `stateIngestionSnapshots.ts`: dispatch from event payloads to snapshot update helpers
  and snapshot replacement from `ClientSnapshot`.
- `stateIngestionTaskNavigation.ts`: project-filtered Task Navigation updates and task
  summary upsert.
- `stateIngestionTask.ts`: Task snapshot updates, chat append/chunk handling, text-part
  coalescing, pending request upsert, and Task-scoped request filtering.

Do not change public type/export names, resync reasons, cursor behavior, project-filtered
task navigation behavior, snapshot replacement behavior, chat append/chunk behavior,
pending request upsert behavior, typed protocol payload usage, or tests.

Focused tests:

- Existing `packages/app-server-client/src/stateIngestion.test.ts` remains the behavior
  suite for moved state-ingestion behavior.
- `npm run check --workspace @openaide/app-server-client` covers TypeScript type-boundary
  safety.

## Implementation

Implemented the split by moving pure snapshot update behavior into focused modules.
`stateIngestion.ts` remains the public ingestion API and still owns scope matching,
cursor validation, resync classification, result construction, and cursor advancement.

Production source sizes after split:

- `stateIngestion.ts`: 88 lines.
- `stateIngestionSnapshots.ts`: 76 lines.
- `stateIngestionTask.ts`: 95 lines.
- `stateIngestionTaskNavigation.ts`: 25 lines.
- `stateIngestionTypes.ts`: 13 lines.

## Review

`$doomsday-review`:

- Correctness: no findings.
- Requirements/tests: accepted one Low missing-test finding for moved `requestUpdated`,
  `taskSnapshotUpdated`, and `missingChatItem` resync paths.
- Code quality: local pass found no findings.

Fix:

- Added focused state-ingestion assertions for missing chat item resync, full Task snapshot
  replacement, and pending request insert/replace behavior.

## Verification

Focused checks already run:

- `npm run check --workspace @openaide/app-server-client`: pass.
- `npm run test --workspace @openaide/app-server-client`: pass.

Final checks:

- `npm run check`: pass.
- `npm test`: pass.
- `git diff --check`: pass.
- `jq empty .workflow/finish-refactor-plan/state.json`: pass.
- Changed production source-size scan: largest split file is `stateIngestionTask.ts` at
  95 lines.

## Commit

This commit: `refactor: split app server client state ingestion`.

## Next

After this slice is committed, select the next compact refactor slice from the current
plan and architecture/file-size pressure.
