# Native Session recovery implementation plan

Status: implemented
Research: [`docs/research/acp-task-open-recovery.md`](research/acp-task-open-recovery.md)

## Outcome

Opening a saved Task must restore editable Agent controls without waiting for Native Session discovery. Once `session/resume` has made an attachment live, OpenAIDE must never start an automatic `session/load` on that attachment.

The product behavior is:

- Native Session discovery, normal live `session/update` projection, and `session/resume` remain automatic.
- A successful resume makes Configuration Options and Send available as soon as the Agent returns them.
- A later listing observation that may indicate external activity records a durable reload requirement. It does not close or load the live attachment.
- While the Task remains open, a compact inline notice immediately above Composer says **This Task may have changed elsewhere.** Supporting copy says **Reload to refresh Chat and Agent options.** The one action is **Reload**. The click itself is the explicit approval; there is no modal or second confirmation.
- If the user does not reload, the next Task open performs `session/load` before controls become editable. That open uses load instead of resume, never both.
- While load runs, stored Chat remains visible, controls and Send are locked, and the existing timeline status says **Reloading session**. Success may briefly say **History updated**.

## Non-goals

- Do not add incremental Chat merge. ACP v1 supplies no portable history revision or replay cursor.
- Do not infer that `SessionInfo.updatedAt` proves new messages. It is only a possible-staleness hint.
- Do not reapply the pre-load Configuration Option catalog after load. The load response is authoritative for the new attachment.
- Do not rename the existing `historySync` wire field in this change. A broader terminology migration would add churn without making recovery safer.
- Do not move recovery policy, timers, persistence, or stale-result decisions into Frontend.
- Do not complete unrelated Native Session Catalog pagination redesign as part of this fix.

## Approved implementation scope

`docs/task-chat-flow.md` is an accepted specification. The implementation was approved across these public seams and requirements:

1. **App Server Protocol seam:** `task/open`, a new `task/reloadNativeSession` mutation, Task snapshots, and Task subscription events.
2. **Agent seam:** the existing Agent runtime interface for `session/resume`, `session/load`, `session/close`, `session/list`, Configuration Options, and live session updates.
3. **Frontend intent seam:** one reload intent through the central Task mutation layer.
4. **Task Page seam:** one persistent inline recovery notice directly above Composer.
5. **Recovery policy:** automatic checks, explicit mid-attachment load, automatic next-open load-first when a durable requirement exists.

The Native Session Synchronization section of `docs/task-chat-flow.md` records the accepted production behavior.

## Invariants

1. One recovery attempt performs exactly one restore operation: resume or load.
2. `session/list` never gates resume of a known bound Native Session.
3. A successfully resumed attachment is never automatically loaded later.
4. A full load starts only before controls are exposed on Task open, or from the explicit Reload intent.
5. The App Server owns the reload requirement, operation ordering, persistence, and visible snapshot state.
6. The Native Session Catalog remains the sole owner of listing observations and pagination.
7. `updatedAt` is treated as a hint; missing, invalid, or incomparable activity never requests destructive replay.
8. Reload shares the existing per-Task operation gate with Send, steering, Configuration Option changes, commands, cancellation, and Archive.
9. If reload wins the gate, later user operations wait and run against the loaded attachment. If another operation wins first, reload is rejected or deferred without touching the attachment.
10. Delayed work from an older attachment epoch cannot update the Task or clear a newer reload requirement.
11. An unknown transport outcome is not retried automatically.
12. Diagnostics contain identifiers, classifications, counts, and durations only; never prompts, Chat, option values, paths, URLs, or free-form Agent errors.

## Target module design

### Native Session Catalog

The Catalog is the deep module for discovery. Its interface should let recovery request non-blocking interest in one known identity while hiding page traversal, cursor lifetime, demand coalescing, and persistence.

Suggested interface shape:

```text
observe_bound_session(agent_id, workspace_root, session_id) -> returns immediately
cached_entry(agent_id, session_id) -> optional persisted observation
```

Task open must not call `AgentGateway::list_sessions` or walk cursors. Catalog demand for the same Agent and canonical Task Workspace must coalesce with Navigation demand. Committed pages notify recovery about matching owned Tasks.

Use `NativeSessionCatalog::entry` as the one cached metadata source. Remove the second listing cache currently held by `HistorySyncCoordinator`.

### Native Session Recovery

Deepen the current `HistorySyncCoordinator`, Task-open worker, and `NativeSessionService::refresh_history` cluster into one recovery module. Callers should not choose close/resume/load ordering themselves.

Suggested interface shape:

```text
recover_on_open(task) -> asynchronous resume-or-load decision
note_catalog_observation(task_id, observation) -> optional durable reload requirement
reload_native_session(task_id, client_mutation_id) -> authoritative Task snapshot
```

The implementation owns:

- one restore choice per attachment;
- transient recovery generation and attachment epoch;
- the existing per-Task operation gate;
- durable reload requirement comparison and conditional clearing;
- close/load/attach/commit ordering;
- history-sync snapshot publication;
- safe terminal diagnostics.

### Attachment epoch

Create a process-local monotonically increasing attachment epoch for each Task recovery. Resume, load, replacement, close, and Archive advance it. Resume/load workers and attached Agent update consumers carry the epoch they belong to. Before committing a recovery result or live update, validate that its epoch still owns the Task's bound Agent/session pair.

This epoch is an internal seam. Do not expose it through App Server Protocol or persist it.

## Durable data model

Add one optional Task record value:

```text
TaskNativeSessionReloadRequirement {
  observed_activity_at: String
}
```

Suggested field: `native_session_reload_requirement: Option<TaskNativeSessionReloadRequirement>`.

Rules:

- Deserialize missing values as `None`; no bulk migration is required.
- Store only a validated, normalized Agent activity timestamp.
- Advance it monotonically when a newer comparable listing observation exceeds local replayed Chat activity by the accepted tolerance.
- Do not use discovery time as Agent activity.
- Preserve it through process restart, Archive, and Restore.
- Clear it after a successful authoritative load only if the stored requirement has not advanced beyond the timestamp captured when that load started.
- Prepared Tasks and newly adopted Tasks start with no requirement.
- Reset Task History removes it with the Task.
- Keep it independent from `native_session_data_freshness`; that value describes whether process-local controls are live, not whether replay is deferred.

Add storage round-trip and legacy-record tests in `openaide-rs/app-server/src/storage/tests.rs`.

## App Server Protocol changes

### Snapshot state

Extend `TaskHistorySyncSnapshot` with:

```text
ReloadAvailable { generation }
```

Keep the existing `Idle`, `Syncing`, and `Updated` states for compatibility with current reconciliation and presentation behavior.

State meanings:

| State | Meaning | Composer |
| --- | --- | --- |
| `idle` | No known reload requirement | Normal |
| `reloadAvailable` | A possible external change is durably recorded; no load is running | Remains usable |
| `syncing` | Explicit or open-time load owns the Task gate | Locked |
| `updated` | Authoritative load committed | Normal; brief announcement allowed |

On process restart, a durable requirement projects as `reloadAvailable` even though the process-local generation restarts.

### Mutation

Add:

```text
task/reloadNativeSession

TaskReloadNativeSessionParams {
  taskId: TaskId,
  clientMutationId: ClientMutationId
}

TaskReloadNativeSessionResult {
  task: TaskSnapshot
}
```

Mutation contract:

- Valid only for an Open, idle Task with a bound Native Session and a current reload requirement.
- Reject a Running, Starting, Archived, tombstoned, missing, or superseded Task without closing anything.
- Deduplicate the same in-process `clientMutationId`.
- Never retry after an unknown transport outcome.
- Publish `syncing` before close/load begins.
- On success, return and publish the loaded authoritative Task snapshot.
- On definite failure, keep the requirement, return to `reloadAvailable`, retain stored Chat, mark controls stale when the old attachment cannot be reused, and surface an inline retryable error.

Update Rust declarations, method names/maps, routing, handlers, protocol tests, then run `npm run protocol:generate` and `npm run protocol:check` to regenerate TypeScript bindings.

## Backend lifecycle changes

### Task open without a pending requirement

```text
task/open
  -> return stored Task immediately with controls recovering
  -> recovery gate selects resume
  -> session/resume(bound id)
  -> apply returned catalogs and attach the session update consumer
  -> publish controls ready
  -> ask Catalog for a non-blocking bound-session observation
```

Remove `refresh_bound_session_for_open` from this path. Listing failure, omission, or slowness must not affect controls readiness.

### Task open with a pending requirement

```text
task/open
  -> return stored Task immediately with controls recovering
  -> recovery gate selects load
  -> close old process attachment when present and supported
  -> session/load(bound id), including replay
  -> atomically replace Chat and Agent-owned session projection
  -> attach update consumer and clear the captured requirement
  -> publish controls ready and history updated
```

Do not resume first. If load is unsupported or definitely fails, retain the requirement and follow the existing stale-control recovery path rather than silently resuming and pretending replay succeeded.

### Later Catalog observation

Replace `spawn_subscribed_task_history_refresh` and its automatic `refresh_history` call with a metadata-only commit:

```text
listing observation newer than local replayed Chat
  -> validate Task still owns Agent/session/workspace
  -> advance durable reload requirement
  -> publish reloadAvailable
  -> do not close, resume, or load
```

Repeated observations coalesce by the maximum validated activity timestamp. Listing omission or failure changes nothing.

### Explicit reload

Run the new mutation under `TaskOperationCoordinator::serialize`:

1. Re-read and validate the Task and requirement under the gate.
2. Capture the requirement timestamp and attachment epoch.
3. Publish `syncing`; mark cached controls recovering and Send blocked.
4. Detach and close the current attachment when supported.
5. Load once and collect replay plus the returned catalogs.
6. In one Task mutation, replace replayable Chat and Agent-owned session projection.
7. Clear only the captured-or-older durable requirement.
8. Attach the permanent session update consumer, advance the attachment epoch, and publish `updated`.

Configuration Option mutations that arrive after reload acquired the gate must run afterward against the loaded attachment. They therefore become the final visible state.

## Frontend changes

### Mapping and reconciliation

- Map `reloadAvailable` through `appServerProtocolMapping.ts`.
- Extend history-sync reconciliation without merging it with durable Task revision. Preserve the existing process-epoch rules for generation reset.
- Do not synthesize reload availability from timers, connection state, option state, or Chat timestamps.
- Add a central `reloadNativeSessionIntent` in `taskMutationIntents.ts`; rendering modules receive only a callback.
- The intent creates a stable client mutation id, does not retry, maps the returned snapshot, and keeps a definite failure local to the Task recovery notice.

### Task Page presentation

Create a small `TaskSessionReloadNotice` rendering module and place it in `TaskView` after workspace/queue surfaces and immediately before `AgentRecoveryPanel` or Composer.

Default content:

```text
This Task may have changed elsewhere.
Reload to refresh Chat and Agent options.                 [Reload]
```

Presentation rules:

- Persistent inline surface, not a toast, modal, Chat message, or Navigation badge.
- `role="status"`; the button has the accessible name **Reload Task from Agent**.
- Keep Composer and Configuration Options usable in `reloadAvailable`.
- Hide or disable the action while the Task is active, disconnected, archived, or already syncing; the durable requirement remains.
- On request start, rely on the authoritative `syncing` snapshot to lock controls. A small local pending state may disable double-clicks until that snapshot arrives.
- On definite failure, retain the notice, show concise inline error text, and restore the Reload button.
- At narrow width, stack the action below the copy without horizontal overflow. Do not add a modal at any viewport.
- Reuse semantic warning/surface tokens and existing button vocabulary. Do not add a decorative card or side-stripe alert.

## Test-first vertical slices

Follow one red, one minimal green, then the next red. Do not write all tests before implementation.

### Slice 0: accepted behavior and seams

- Update `docs/task-chat-flow.md` with one-restore-per-attachment, deferred later replay, explicit Reload, and next-open load-first behavior.
- Record the exact App Server Protocol and Task Page seams above.
- Stop if product approval changes any of these requirements.

### Slice 1: safe fast-open backend tracer

Primary seam: `TaskOpenWorkflow` exercised with the existing recording Agent adapter.

First red test:

- Block multi-page listing indefinitely.
- Open a saved Task with resume support.
- Assert the stored snapshot returns and resume begins without any list result.
- Assert no load occurs.

Minimal green:

- Remove list-before-resume ordering.
- Choose direct resume when no durable requirement exists.
- Preserve cached-control recovering/ready transitions.

Second red test:

- After resume, publish a newer listing observation.
- Assert close/load remain zero and a reload requirement is recorded.

Do not deploy this slice until Slice 2 exposes recovery for the deferred requirement.

### Slice 2: durable requirement and protocol projection

Primary seam: Task snapshot and subscription event.

Red-green cycles:

1. A newer valid observation projects `reloadAvailable` without replacing Chat.
2. A missing, invalid, equal, or older timestamp leaves state unchanged.
3. A later observation advances the requirement monotonically.
4. Process restart preserves and projects `reloadAvailable` with a new process-local generation.
5. Agent/session/workspace mismatch cannot affect another Task.

### Slice 3: explicit reload mutation

Primary seam: typed `task/reloadNativeSession` request and Task subscription.

Red-green cycles:

1. Reload publishes `syncing`, closes/loads exactly once, replaces replay, and publishes `updated`.
2. A queued Configuration Option mutation runs after load and remains final.
3. A user operation that owns the Task gate first prevents reload from touching the attachment.
4. A repeated client mutation id does not cause another close/load.
5. A definite load failure retains `reloadAvailable` and existing Chat.
6. A stale result from an older attachment epoch cannot commit.
7. A newer requirement observed during load is not cleared by the older load.

Use the Agent adapter only to arrange ACP responses, blocking, replay chunks, and live updates. Assert behavior through the workflow/protocol result and published Task state rather than private locks.

### Slice 4: next-open load-first

Primary seam: `task/open`.

Red-green cycles:

1. A Task with a durable requirement calls load without calling resume or list.
2. Controls remain recovering until load commits.
3. Success clears the captured requirement and attaches one live update consumer.
4. Unsupported or failed load keeps recovery visible and never falls through to a misleading resume.

### Slice 5: Frontend intent and presentation

Primary seams: central Task intent and rendered `TaskView`.

Red-green cycles:

1. `reloadAvailable` renders the persistent notice directly above Composer while Composer remains usable.
2. Clicking Reload sends one typed mutation with one stable client mutation id.
3. `syncing` removes/disables the action and locks controls through authoritative snapshot state.
4. A definite error leaves the notice retryable and does not erase the Composer draft.
5. `updated` preserves the existing brief announcement and Chat paging replacement behavior.
6. Active, archived, disconnected, and narrow-window states have the specified affordance.

Do not test CSS literals or selectors. Test visible copy, button accessibility, intent dispatch, and Composer availability.

### Slice 6: Catalog ownership and coalescing

Primary seam: Native Session Catalog observation demand.

Red-green cycles:

1. Task open registers background interest but never calls listing directly.
2. Task and Navigation demand for the same Agent/Task Workspace share one live page stream.
3. Each successful page can satisfy a watched bound identity immediately.
4. Failure preserves the last committed Catalog and Task requirement.
5. No-new-identity page termination and untrusted ordering follow the existing Catalog contract.

Remove the duplicate `NativeSessionCache` only after these tests pass.

### Slice 7: observability and cleanup

- Add structured start and terminal events for open recovery, Catalog observation, deferred reload, explicit reload, load commit, and attachment.
- Add list-page and process-operation mutex wait timing so future gaps are attributable.
- Remove obsolete open-time targeted scan and automatic subscribed-history replacement paths.
- Keep production files below the repository source-size limit by extracting the cohesive recovery module rather than extending `product_api/open.rs`.

## Expected file areas

Backend and protocol:

- `docs/task-chat-flow.md`
- `openaide-rs/app-server-protocol/src/task.rs`
- `openaide-rs/app-server-protocol/src/snapshot/task.rs`
- `openaide-rs/app-server-protocol/src/methods.rs`
- `openaide-rs/app-server-protocol/src/methods/names.rs`
- `openaide-rs/app-server/src/storage/records.rs`
- `openaide-rs/app-server/src/storage/tests.rs`
- `openaide-rs/app-server/src/tasks/product_api/open.rs`
- `openaide-rs/app-server/src/tasks/product_api/list_sessions.rs`
- `openaide-rs/app-server/src/tasks/history_sync.rs` or its recovery-module replacement
- `openaide-rs/app-server/src/tasks/native_session_service/open_recovery.rs`
- `openaide-rs/app-server/src/tasks/turn_events.rs` and session update projection
- `openaide-rs/app-server/src/protocol_edge/routing.rs`
- `openaide-rs/app-server/src/protocol_edge/task_handlers.rs`
- existing product/protocol/storage tests adjacent to those modules

Generated client and Frontend:

- `packages/app-server-client/src/generated/protocol.ts` through the generator only
- `packages/frontend/src/state/appServerProtocolMapping.ts`
- `packages/frontend/src/state/taskSnapshotReconciliation.ts`
- `packages/frontend/src/intents/taskMutationIntents.ts`
- `packages/frontend/src/components/taskCallbacks.ts`
- `packages/frontend/src/components/AppPrimaryTaskSurface.tsx`
- `packages/frontend/src/components/TaskView.tsx`
- a focused `TaskSessionReloadNotice.tsx` and stylesheet section
- existing mapping, reducer, intent, and TaskView presentation tests

## Verification

Run the narrowest check after each red-green cycle, then broaden:

1. Focused Rust test by exact new test name in `openaide-app-server`.
2. `cargo test -p openaide-app-server-protocol` after protocol records change.
3. `npm run protocol:generate` followed by `npm run protocol:check`.
4. Focused Frontend Vitest files for mapping, reconciliation, intents, and TaskView presentation.
5. `cargo test -p openaide-app-server`.
6. `npm run check`.
7. Browser verification in both a wide editor-like viewport and a narrow viewport. Exercise notice visibility, Reload, loading lock, failure retry, option selection before reload, and Composer draft preservation.

Because this is OpenAIDE self-development, use the disposable Target for runtime verification. Do not rebuild or restart the Driver. Do not deploy or restart Target unless the user explicitly requests it in that turn; otherwise report that browser/runtime verification is still pending.

## Rollout and diagnostics

Add these metadata-only events before judging latency:

- `native_session_open_recovery_decided`: task/agent ids, decision `resume|loadDeferred|loadCached`, attachment epoch.
- `bound_session_observation_started|completed`: safe ids, page count, found/exhausted/failed/coalesced, duration.
- `agent_process_operation_wait_completed`: operation class and wait duration.
- `native_session_reload_deferred`: safe ids, generation, classified trigger.
- `native_session_reload_started|completed`: safe ids, generation, outcome, close/load/commit/attach durations.
- `native_session_reload_requirement_preserved`: classified reason such as `newerObservation|loadFailed|staleEpoch`.

Target acceptance evidence:

- In the original slow-session shape, `native_session_open_resume_started` begins without a preceding bound-session scan and controls settle near resume latency.
- A post-resume Catalog observation produces `reloadAvailable` and zero automatic load calls.
- Selecting a Configuration Option before the notice appears is never silently reverted.
- Clicking Reload performs one load and makes its result authoritative.
- Reopening with a deferred requirement performs load-first.

## Risks and mitigations

| Risk | Mitigation |
| --- | --- |
| `updatedAt` advances for config/title rather than Chat | Use possible-change copy, require explicit mid-session Reload, never claim new messages |
| User ignores the notice and reads stale Chat | Persist the requirement; next open loads before interaction |
| Reload changes visible Configuration Options | The action copy names Agent options; click is explicit approval; never auto-load after resume |
| Load is unsupported | Retain requirement and stored Chat; return a definite capability error and keep recovery visible |
| Load fails after close | Keep Chat, mark controls stale, retain retry action, and report classified failure |
| New observation races with load | Clear only the captured-or-older requirement; preserve a newer requirement |
| Old worker or update commits after replacement | Validate attachment epoch and bound Agent/session identity before every commit |
| Duplicate Reload click or response | Local pending disable plus server-side client mutation deduplication |
| Global discovery remains slow | It no longer gates controls; page/mutex diagnostics and demand coalescing address resource cost separately |
| Existing clients see a new snapshot variant | Regenerate typed bindings and update exhaustive mapping/reconciliation before protocol check passes |

## Completion criteria

- The accepted specification describes the new policy.
- A known Task resumes without waiting for `session/list`.
- No code path automatically loads a successfully resumed attachment.
- Possible external activity becomes a durable `reloadAvailable` state.
- The inline Reload action works through the central intent layer and typed protocol.
- The next open with a deferred requirement loads first and never resumes first.
- Configuration Option and live Agent updates cannot be overwritten by delayed recovery work.
- Catalog listing has one owner and overlapping demand coalesces.
- Required diagnostics explain every recovery wait and terminal outcome.
- Focused tests, protocol generation/check, full relevant backend/frontend checks, and wide/narrow browser verification pass.
- No Driver runtime or assets were mutated during implementation or verification.
