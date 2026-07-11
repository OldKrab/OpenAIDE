# A3c Task Send API

## Contract

Implement the first App Server Protocol `task/send` path for already-created
Tasks.

- Accept typed `taskId`, `idempotencyKey`, `taskRevision`, and part-based
  composer message input.
- Commit a durable user message and running turn marker for an idle Task.
- Return `TaskSendResult` with renderable Task snapshot, `turnId`, and
  `userMessageId`.
- Reject stale revisions, active Tasks, blank text, and attachments until the
  attachment runtime exists.
- Preserve non-empty prompt text exactly.
- Persist explicit idempotency-key receipts so retries return the accepted
  `turnId` and `userMessageId`.
- Publish accepted mutation events through state sync to subscribed clients.
- Keep ACP Agent execution as a later readiness/runtime sub-slice.

## Status

Completed.

## Implementation

- Added `TaskSendWorkflow` and `TaskSendAccepted`.
- Split protocol Task handlers into `protocol_edge::task_handlers`.
- Added `storage::send_receipts` for durable `task/send` idempotency receipts.
- Wired `task/send` through the protocol edge and stdio runtime path.
- Added gateway event deliveries and stdio `app/event` notifications for
  subscribed clients.
- Updated Task snapshots so idle Tasks report send capability as ready and
  running Tasks report a task-running blocker.
- Routed create/send Task record commits through `TaskMutations` so revision
  assignment, message backup/restore, Task writes, notification, and response
  snapshots share the existing mutation boundary.

## Review

Round 1 found accepted issues:

- Partial send writes could survive a failed Task commit. Fixed by routing the
  Task record commit through `TaskMutations` and restoring receipts on commit
  failure.
- Retry could recover the wrong turn from prior history. Fixed with explicit
  durable send receipts.
- Accepted mutations discarded state-sync deliveries. Fixed by carrying event
  deliveries in `GatewayOutcome` and serializing stdio notifications.
- Snapshots still said `task/send` was unavailable. Fixed send capability
  projection.
- Non-empty prompts were trimmed. Fixed to preserve text while still rejecting
  whitespace-only text.
- Active-turn rejection lacked coverage. Added a regression test.
- Product API mutations initially had a separate durable mutation path. Fixed
  by reusing the shared `TaskMutations` commit boundary.

## Verification

- `cargo fmt --all`
- `cargo check -p openaide-runtime`
- `cargo test -p openaide-runtime tasks::product_api -- --nocapture`
- `cargo test -p openaide-runtime protocol_edge -- --nocapture`
- `cargo test --workspace -- --test-threads=1`
- `npm run check`
- `npm run test --workspace @openaide/app-server-client`
- `git diff --check`
- `jq empty .workflow/finish-refactor-plan/state.json`

## Next

Commit this sub-slice before the next A3 workflow.
