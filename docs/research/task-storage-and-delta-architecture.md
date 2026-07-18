# Task storage and semantic delta architecture

Research date: 2026-07-17

## Conclusion

The durable destination should be a deep **Task State Store Module** backed by SQLite in WAL mode, fed by a bounded **Native Session Delta Pipeline Module**. The pipeline converts ACP updates into a small, explicit semantic delta algebra—append, patch, replace, and lifecycle barriers—then commits short ordered batches. One SQLite transaction appends the normalized events, updates the materialized Task/Chat/Tool projections, advances the Task revision once, and returns publication facts. App Server publishes only after that transaction commits.

This is not “SQLite instead of batching.” SQLite fixes atomicity, indexed mutation, recovery, multi-process access, and full-file rewrite costs; semantic batching fixes the 800-updates-per-second workload that would otherwise become 800 SQLite transactions and 800 Task revisions. Both are required.

Use the existing per-Task append journal as a temporary bridge, not the final architecture. Extending it to tool upserts and detail appends can remove the immediate whole-history rewrite while SQLite is introduced. An in-memory coalescer alone is an incomplete fix: it lowers frequency but still repeatedly rewrites `messages.jsonl`, tool artifacts, metadata, and `task.json`, and it retains the global mutation lock and custom multi-file rollback protocol.

The recommendation preserves the accepted invariants:

- one Native Session update consumer processes updates in arrival order;
- acknowledged Chat and Tool detail changes are durable before publication;
- one durable Task transaction produces one Task revision and one ordered `taskChanged` event;
- authoritative baselines replace a replica after a revision gap;
- Tool details remain durable without subscribers;
- Task Navigation changes only for Navigation-visible summary changes;
- storage failure stops non-durable live rendering rather than pretending memory state will survive.

It follows the already accepted storage direction in [ADR-0022](../adr/0022-backend-frontend-app-shell-architecture.md): core mutable product state belongs in SQLite, direct SQL and explicit transactions are preferred, writes are serialized through a storage executor, WAL and busy timeouts arbitrate local access, and the durable source is normalized Chat events with a materialized render state. [ADR-0023](../adr/0023-task-state-publication-and-replica-recovery.md) supplies the revision, focused-publication, Navigation, and baseline-recovery rules. [ADR-0025](../adr/0025-task-frontend-boundaries.md) keeps Native Session recovery and update consumption behind a deep App Server Module rather than exposing storage choices to Frontend.

There is an accepted-document conflict to resolve before implementation: the newer [ADR-0024](../adr/0024-task-chat-persistence.md) names `messages.jsonl` and a per-Task append journal as the concrete Chat store, while ADR-0022 requires SQLite for normalized Chat events and materialized Chat render state and rejects plain JSON for core mutable product state. This note recommends resolving that conflict explicitly in favor of ADR-0022's SQLite destination while preserving ADR-0024's behavioral requirements: append without full-history rewrite, durable-before-publish commits, crash-safe materialization, and a dedicated local Chat clock. No accepted specification or ADR is changed by this research note.

## Incident evidence and current write amplification

The profiled incident contained 10,002 updates for one command: 10,000 terminal-output deltas plus status updates. They arrived over about 12.4 seconds and contained 372,861 output characters. The affected Task had a 4,402,392-byte `messages.jsonl`. During the same period, tiny text commits for unrelated Tasks took roughly 375–505 ms. The Firefox profile was mostly idle; repeated HTTP `/probe` requests were the resumable event receive loop exposing frequent `taskNavigationChanged` frames, not the CPU cause ([captured profile](https://share.firefox.dev/4wgjtQ8)).

The trace also proves a normalization failure. Each high-volume update carried `{terminal_id, data}` under implementation-specific `_meta.terminal_output_delta`. The current Tool projection maps standard `content`, locations, raw input, and raw output but not that `_meta` extension ([`tool_details.rs`](../../openaide-rs/app-server/src/agent/tool_details.rs#L94-L126)). The stored final Tool row had no detail artifact. OpenAIDE therefore performed thousands of durable mutations for product projections that were effectively unchanged while failing to persist the terminal bytes that caused them. The immediate seam needs both a normalized no-op check and an explicit Adapter for any supported terminal extension; batching alone would merely perform fewer meaningless writes.

The code explains the amplification:

1. Every tool update enters `upsert_session_tool`, requests message-history refresh, mutates `updated_at`, and unconditionally reports `Changed` ([`turn_events.rs`](../../openaide-rs/app-server/src/tasks/turn_events.rs#L285-L305)).
2. `commit_existing_task` takes the process-wide Task storage mutex, reads `task.json`, creates message-file rollback state, executes the mutation, persists it, and only then publishes ([`commit.rs`](../../openaide-rs/app-server/src/tasks/mutation/commit.rs#L19-L88), [`service.rs`](../../openaide-rs/app-server/src/tasks/service.rs#L107-L146)).
3. A tool upsert calls `read_messages`, scans for the identity, then rewrites and syncs the complete `messages.jsonl` plus metadata ([`message_store.rs`](../../openaide-rs/app-server/src/storage/message_store.rs#L51-L85)). Reading also parses the entire materialized file and replays its journal ([`message_store.rs`](../../openaide-rs/app-server/src/storage/message_store.rs#L109-L141)).
4. Full Tool detail is separately rewritten as one pretty-printed atomic JSON artifact ([`tool_artifacts.rs`](../../openaide-rs/app-server/src/storage/tool_artifacts.rs#L26-L105)).
5. The commit increments `task_version` and Task revision, recomputes projections, and rewrites `task.json` ([`commit.rs`](../../openaide-rs/app-server/src/tasks/mutation/commit.rs#L501-L547)). Because both `updated_at` and `message_history_version` count as summary changes, the same byte-level activity produces Navigation upserts ([`commit.rs`](../../openaide-rs/app-server/src/tasks/mutation/commit.rs#L619-L665)).

Agent text has a useful partial optimization: `AppendMessage` and `AppendText` journal records plus a Task-local identity index avoid rewriting materialized Chat for ordinary text chunks ([`agent_append.rs`](../../openaide-rs/app-server/src/storage/message_store/agent_append.rs#L98-L188), [`journal.rs`](../../openaide-rs/app-server/src/storage/message_store/journal.rs#L20-L152)). Tool updates, non-text message parts, permission outcomes, and activity completion still take the materialized rewrite path. The custom transaction also spans materialized history, an append file, metadata, Tool artifacts, and `task.json`; extending this indefinitely would recreate a transactional database in application code.

At state-root scope, `StorageOpenGuard` also takes an exclusive `storage-writer.lock` for the complete Store lifetime ([`open_state.rs`](../../openaide-rs/app-server/src/storage_runtime/open_state.rs#L11-L29)). That directly conflicts with ADR-0022's accepted multiple-App-Server SQLite model and is another reason a per-process or per-Task mutex refinement cannot be the permanent storage design.

The current file durability path also discards `sync_all` failures with `.ok()` for both atomic replacement files and append-journal records ([`atomic.rs`](../../openaide-rs/app-server/src/storage/atomic.rs#L13-L27), [`journal.rs`](../../openaide-rs/app-server/src/storage/message_store/journal.rs#L114-L152)). A successful method return therefore does not prove that the required sync succeeded. This weakens the stated durable-before-publish invariant independently of the performance bug and should be covered by injected sync-failure tests during migration.

## Protocol constraints and opportunity

ACP v1 tool calls arrive through `session/update` notifications, which OpenAIDE consumes in arrival order. A `tool_call_update` is keyed by `toolCallId`; all other fields are optional and only supplied fields change. Its `content` collection is replacement state, and a terminal reference points to a Client-owned ACP terminal ([official ACP tool-call documentation](https://agentclientprotocol.com/protocol/v1/tool-calls.md), [official terminal documentation](https://agentclientprotocol.com/protocol/v1/terminals.md)). ACP v1 does not standardize Agent-owned `terminal_output_delta`; supported `_meta` extensions therefore need named, fixture-tested Adapters. Unknown extensions that produce no normalized product change must not create Task commits, but OpenAIDE must not guess append/replacement semantics or silently claim their content was persisted.

The current ACP v2 documents are drafts, not an accepted protocol contract, but they are strong design evidence:

- whole-message updates are upserts with tri-state patch fields, while chunks append; a later whole-message replacement can correct or redact earlier content ([v2 Message Updates RFD](https://agentclientprotocol.com/rfds/v2/message-updates.md));
- Tool updates patch fields, Tool content updates replace arrays, and `tool_call_content_chunk` appends one item in receive order ([v2 Tool Call Updates RFD](https://agentclientprotocol.com/rfds/v2/tool-call-updates.md));
- terminal snapshots replace stored bytes while `terminal_output_chunk` appends independently encoded byte chunks in receive order; the RFD explicitly says replacement is needed for replay/correction and append is needed for efficient live output ([v2 Terminal Output RFD](https://agentclientprotocol.com/rfds/v2/terminal-output.md));
- session replay is expected to evolve toward cursor-based `session/resume`, which makes authoritative replace/replay operations first-class ([v2 Session Resume Replay RFD](https://agentclientprotocol.com/rfds/v2/session-resume-replay.md)).

OpenAIDE should not expose ACP objects as its storage or Frontend contract. It should preserve these semantics in typed product deltas so an ACP v2 Adapter can be added without another storage redesign.

## Options compared

| Option | Strengths | Limits | Decision |
| --- | --- | --- | --- |
| In-memory coalescer over the current store | Fastest mitigation; preserves a trailing newest state; can reduce 10,000 callbacks to hundreds of commits. | Every remaining batch still parses/rewrites multi-megabyte history and rewrites growing Tool artifacts; crash recovery and atomicity remain custom; process-wide lock still contains slow I/O. | Add immediately, but only as the front of the final pipeline. |
| Extend per-Task append journals | Reuses the working text-chunk pattern; turns Tool summary/detail changes into sequential appends; can make the bridge performant without changing product protocol. | Requires new record types, indexes, replay, compaction, rollback across several files, cross-process coordination, schema migration, and corruption repair. This duplicates SQLite and conflicts with ADR-0022's selected destination. | Use narrowly as a bridge if direct SQLite migration cannot land soon. Do not deepen it into the permanent store. |
| SQLite WAL with normalized events and materialized projections | Atomic event-plus-projection commits; indexed entity updates; no full-history rewrite; snapshot readers can run while a writer commits; crash recovery and rollback are database responsibilities; fits accepted multi-process architecture. | WAL still permits only one writer at a time, so transactions must remain short. SQLite alone does not reduce update/revision volume. Checkpoint policy and durability settings must be explicit. | Permanent storage Adapter, paired with semantic batching. |

SQLite's official documentation confirms the relevant trade-off: WAL allows readers and a writer to proceed concurrently and makes writes sequential, but there is still only one writer; checkpointing is a distinct operation and long readers can delay it ([WAL documentation](https://www.sqlite.org/wal.html), [transaction documentation](https://www.sqlite.org/lang_transaction.html), [isolation documentation](https://www.sqlite.org/isolation.html)). The design must therefore use short transactions and fair scheduling, not claim parallel writes that SQLite does not provide.

## File-journal prototype benchmark

A disposable logic prototype under `tmp/prototypes/task-delta-storage/` models the non-SQL alternative: fixture-tested normalization of the observed terminal extension, semantic no-op removal, append/replace/patch operations, 32 ms/64 KiB/256-operation batching, one synced per-Task delta-journal record per batch, focused Navigation derivation, replay, and one prompt-boundary materialization. Run it from the repository root with:

```sh
node tmp/prototypes/task-delta-storage/benchmark.mjs
```

The benchmark uses scratch files on the same ext4 filesystem as the repository and deletes them after each run. The current-path model performs the operations implicated by the incident: hard-link rollback state; read and parse all materialized Chat; replace the unchanged Tool projection; atomically rewrite and sync `messages.jsonl`, message metadata, and Task metadata; then publish one revision and Navigation change. The proposed-path model executes all 10,002 incident-shaped updates and 372,861 terminal characters, replays the result, and materializes 4,402,392 bytes of Chat plus the terminal artifact once.

The sustained validation used 250 real current-path commits at each history size. At the incident shape it produced:

| Measurement | Current rewrite | Proposed file journal, 32 ms |
| --- | ---: | ---: |
| Raw updates represented | 250 measured; 10,002 projected | 10,002 measured |
| Existing Chat | 4,402,392 bytes / 2,426 rows | same |
| Mean storage service time | 28.23 ms per raw commit | 0.90 ms per batch |
| p95 storage service time | 32.40 ms per raw commit | approximately 1–1.5 ms per batch across repeated runs |
| Durable batches / Task revisions | 10,002 projected | 386 measured |
| Output-only Navigation events | 10,002 projected | 0 measured |
| Storage service time for incident | 282.36 s projected | 0.36 s measured, including compaction |
| Bytes written for incident | 41.01 GiB projected | 4.97 MiB measured |
| Syncs for incident | 30,006 projected | 388 measured, including compaction |

The 250-commit current run wrote 1.10 GB and completed in 7.06 seconds. Current cost scaled with history size: approximately 7.03 ms at 0.5 MiB, 16.62 ms at 2 MiB, and 28.23 ms at 4.2 MiB. This supports the `O(history bytes × update count)` model. Executing all 10,002 current commits would intentionally churn about 44.0 billion bytes, so its time is extrapolated from the sustained run; the proposed result executes the complete workload. Because the captured updates arrived over 12.4 seconds, these figures are storage service demand, not user-visible elapsed time: the current path requires roughly 282 seconds of isolated work for 12.4 seconds of input and must accumulate backlog, while the proposed path requires less than 3% of that arrival window and can keep up.

Five shorter repeated ext4 runs put the 4.4 MB current mean between 29.65 and 39.18 ms and the complete proposed 32 ms policy between 0.326 and 0.374 seconds including compaction. The prototype also compares batching policies:

| Maximum batch age | Durable batches | Approximate storage service time including compaction |
| ---: | ---: | ---: |
| 8 ms | 1,430 | 1.17–1.20 s |
| 16 ms | 771 | 0.63–0.67 s |
| 32 ms | 386 | 0.32–0.37 s |
| 50 ms | 245 | 0.21–0.26 s |
| 100 ms | 125 | 0.11–0.15 s |

The 32 ms starting point stays comfortably inside the accepted 96 ms live-presentation target while leaving substantial storage headroom. Final defaults still need a Rust integration benchmark on the production reducer and slow-filesystem fault injection.

Sixteen deterministic corner cases pass, including stale Session fencing, unknown and empty extension no-ops, exact observed-terminal preservation, append/replace ordering, interleaved identities, a Unicode scalar split across chunks, prompt-completion and late-update barriers, an oversized delta, torn-tail recovery, failed partial append recovery, crash between compaction and journal deletion, visible corruption, and Navigation filtering. A seeded differential check additionally compares raw replay with batched replay for 500 generated streams of 120 mixed operations each.

This materially changes the decision boundary: SQLite is not required for throughput. The file-journal design can solve the measured performance and backlog problem by several orders of magnitude. SQLite remains the recommended permanent Adapter only if OpenAIDE retains atomic cross-record updates, multiple App Server writers, indexed queries, migrations, and recovery requirements that would otherwise grow the custom journal into a database. If those requirements are narrowed, the benchmark supports keeping a carefully specified per-Task journal.

## Recommended Modules and seams

### Native Session Delta Pipeline Module

This Module owns the ordering, normalization, batching, backpressure, and flush-barrier policy for one Native Session. Its external Interface stays small:

```text
accept(orderedNativeUpdate) -> Accepted | Backpressured | Failed
barrier(reason)              -> DurableReceipt | Failed
close(reason)                -> DurableReceipt | Failed
```

`accept` assigns a process-local source ordinal immediately and never asks callers how to merge or persist an update. `barrier` means every earlier accepted ordinal has committed. Prompt completion, shutdown, session replacement, and history-baseline installation use this operation. The Module's depth comes from hiding semantic reduction, timer/size thresholds, fair storage scheduling, retries, and trailing-flush guarantees behind those three operations.

The ACP seam contains concrete `AcpV1TaskDeltaAdapter` and future `AcpV2TaskDeltaAdapter` Adapters. They map protocol-specific payloads to the same product delta types. A deterministic fixture Adapter is justified for tests, so this is a real seam rather than hypothetical indirection.

### Task State Store Module

This Module owns normalized events, Task metadata, materialized Chat, Tool details, versions, SQL transactions, projection rebuild, and query paging. Its Interface is:

```text
commit(TaskDeltaBatch) -> Committed(CommitReceipt) | Unchanged
snapshot(TaskSnapshotQuery) -> TaskSnapshot
chatPage(ChatPageQuery) -> ChatPage
toolDetail(ToolDetailQuery) -> ToolDetailSnapshot
```

`commit` is the only mutation operation. Callers do not issue SQL, bump versions, update clocks, choose journals, write artifacts, derive Navigation changes, or publish events. `Unchanged` performs no durable write and produces no revision or publication. `CommitReceipt` contains the committed Task revision and focused publication facts. The caller publishes that receipt after return; a failed commit has no publication facts.

The SQLite implementation is the production Adapter. An in-memory implementation is useful only if it executes the same semantic reducer and invariants; a fake that bypasses ordering, patch/replace semantics, or atomic projection updates would hide the bug class this Module exists to prevent.

## Semantic delta model

Keep the algebra explicit and typed. Do not store raw ACP envelopes or a generic JSON Patch language.

```text
TaskDeltaBatch {
  taskId
  expectedBinding { agentId, nativeSessionId, ownershipEpoch }
  sourceOrdinalRange
  orderedDeltas[]
  commitReason
}

SemanticDelta =
  ChatItem(ChatItemDelta)
  Tool(ToolDelta)
  Terminal(TerminalDelta)
  Catalog(CatalogReplacement)
  Task(TaskTransition)
  History(AuthoritativeHistoryReplacement)

ChatItemDelta =
  AppendItem(item)
  AppendContent(itemId, content)
  PatchItem(itemId, typedPatch)
  ReplaceItem(itemId, item)

ToolDelta =
  Upsert(toolCallId, typedPatch)
  AppendContent(toolCallId, contentItem)
  ReplaceContent(toolCallId, content[])
  RecordPermission(toolCallId, outcome)

TerminalDelta =
  Patch(terminalId, typedPatch)
  AppendBytes(terminalId, bytes)
  ReplaceOutput(terminalId, bytes)

FieldPatch<T> = Unchanged | Clear | Set(T)
```

Important rules:

1. Entity identity is Task-local and includes the Native Session identity where ACP ids are only session-unique. Tool and Terminal id domains remain distinct.
2. `Append`, `Patch`, and `Replace` are different operations. `Replace` is necessary for authoritative replay, correction, clear, and redaction; `Append` is necessary for efficient streaming.
3. The Adapter preserves omitted, `null`, and concrete values as `Unchanged`, `Clear`, and `Set`. Collapsing omission and clear would make future ACP v2 incorrect.
4. The batch retains original delta order. The reducer folds only provably equivalent operations: adjacent appends to the same entity concatenate; adjacent patches compose field-by-field with last-write semantics; a later replacement supersedes earlier uncommitted state for that same field; later appends apply after the replacement.
5. A reduced event records its source-ordinal range for diagnostics. Ordinals are not a protocol deduplication key. ACP supplies no general live-update sequence, so OpenAIDE must not invent duplicate suppression that could discard legitimate repeated chunks.
6. `AuthoritativeHistoryReplacement` is one explicit baseline operation. It does not fan replay into synthetic live deltas.
7. A normalized update whose semantic projection equals current pending or durable state is a no-op. It does not advance Chat clocks, Task revision, or Navigation and does not rewrite storage.

## Batching, ordering, cancellation, and backpressure

Initial thresholds should be measurements-backed configuration, not product settings:

- start a flush no later than 32 ms after the first pending live delta, with a 50 ms hard age limit;
- flush earlier at 64 KiB of accumulated payload or 256 semantic operations;
- concatenate adjacent Agent/Thought text and terminal bytes before storage, preserving exact byte/text order;
- compose adjacent patches and retain only the final replacement state within the same safe reduction window;
- always perform a trailing flush when the timer fires, the stream goes quiet, or the pipeline closes.

These starting values leave room inside the accepted 96 ms Frontend catch-up target while reducing an 800-update/second stream to at most tens of durable commits per second. They must be benchmarked on slow disks and adjusted from observed commit latency and queue age.

Immediate barriers are required for:

- accepted User message and first-Send promotion;
- Permission or Question resolution;
- Task lifecycle/status transitions, including `stopping`;
- prompt response completion before publishing idle;
- Native Session replacement or close;
- authoritative history replacement;
- graceful shutdown.

`task/cancel` must not wait behind the low-priority output backlog before sending ACP cancellation or publishing `stopping`. The outbound Native Session control path is independent of the inbound delta queue. The ordered ACP update consumer still applies ACP updates in their receive order, accepts late updates after Stop, and makes the prompt-completion barrier wait for every earlier accepted update as required by the specification.

The queue is bounded by both item count and bytes. At pressure:

1. reduce adjacent equivalent deltas more aggressively without losing final state or ordered bytes;
2. schedule storage fairly across Tasks so one noisy Task cannot monopolize every batch;
3. if the reduced queue still reaches its hard limit, stop reading more Agent output and let the transport apply backpressure rather than allocate unbounded memory;
4. keep cancellation/control writes on their independent path;
5. if storage is unavailable, stop accepting/rendering updates and follow the accepted storage-failure path rather than dropping durable content.

No overflow policy may silently discard terminal bytes, final status, a Permission outcome, or a trailing replacement. Any future terminal retention/truncation cap is a product policy and needs explicit agreement.

## SQLite data model and transaction

A minimal schema shape is:

```text
tasks(
  task_id PK,
  ...authoritative Task metadata...,
  task_revision,
  message_history_version,
  local_history_updated_at
)

task_event_batches(
  task_id,
  task_revision,
  first_event_seq,
  last_event_seq,
  commit_reason,
  committed_at,
  PRIMARY KEY(task_id, task_revision)
)

task_events(
  task_id,
  event_seq,
  event_kind,
  entity_kind,
  entity_id,
  payload,
  source_ordinal_start,
  source_ordinal_end,
  PRIMARY KEY(task_id, event_seq)
)

chat_items(
  task_id,
  item_id,
  timeline_seq,
  kind,
  render_state,
  applied_event_seq,
  PRIMARY KEY(task_id, item_id)
)

tool_details(
  task_id,
  artifact_id,
  detail_state,
  applied_event_seq,
  PRIMARY KEY(task_id, artifact_id)
)

terminal_segments(
  task_id,
  terminal_id,
  segment_seq,
  bytes,
  applied_event_seq,
  PRIMARY KEY(task_id, terminal_id, segment_seq)
)
```

The exact payload columns may use versioned JSON/BLOB encoding initially; direct SQL and typed Rust conversion keep serialization policy inside the Module. Large binary content belongs in bounded segments, not copied into every Chat summary row. Tool summaries reference detail identities. Ordinary Task snapshots read `chat_items`; expanded Tool subscriptions read `tool_details` and terminal segments. Detail writes happen regardless of subscriber count.

One `commit` transaction must:

1. verify Task identity, expected Native Session binding, and ownership epoch;
2. reserve the next per-Task event range and Task revision;
3. append the reduced normalized events in order;
4. apply them to `chat_items`, Tool details, terminal segments, and Task metadata;
5. advance `message_history_version` and `local_history_updated_at` when Chat changed;
6. derive focused Task and Tool-detail publication facts from the actual changed projections;
7. insert the batch/revision record and commit;
8. return the immutable `CommitReceipt`.

Agent stream bytes do not by themselves mutate a Navigation-visible summary timestamp. Navigation publication is derived only from title, lifecycle, status, attention/unread state, archival membership, or another explicitly Navigation-visible field. Chat versions and `local_history_updated_at` remain durable without forcing sidebar upserts. A final lifecycle transition can update Navigation recency once when the product policy calls for it.

Use `journal_mode=WAL`, a finite busy timeout, foreign keys, and short explicit write transactions on the dedicated storage executor. Use `synchronous=FULL` initially: SQLite documents that WAL commits sync on every transaction in FULL mode, while NORMAL omits that commit sync and can lose recent transactions after power failure ([SQLite synchronous pragma](https://www.sqlite.org/pragma.html#pragma_synchronous), [WAL performance notes](https://www.sqlite.org/wal.html#performance_considerations)). Switching to NORMAL would weaken the ordinary meaning of “durable before publish” and requires an explicit product durability decision. Batching is what amortizes FULL-mode commit cost.

WAL checkpointing and semantic event retention are separate policies. Use passive/background checkpoints with metrics for WAL size and checkpoint age; avoid long read transactions. Normalized events remain the rebuild source, while materialized projections carry `applied_event_seq` so startup can detect and repair an incomplete or stale projection. High-volume terminal appends are stored as already-coalesced segments, avoiding 10,000 tiny rows without deleting history.

## Failure and recovery behavior

- SQL errors roll back the complete event-plus-projection transaction. Nothing is published.
- The pipeline retains the uncommitted batch until success or terminal storage failure; it does not acknowledge durable completion early.
- On live storage loss, stop new Agent-update rendering, persist storage-error/interruption state if possible, close or detach the Agent transport as best effort, and do not cleanly release ownership if coherent state cannot be persisted. This matches ADR-0022.
- On restart, validate schema and projection checkpoints. Rebuild a stale materialized projection from normalized events before serving it. A baseline is built from the materialized projection only after it matches the event head.
- History replay installs one authoritative replacement transaction and starts a new ingest epoch. Stable Agent message/tool identities make upsert replay deterministic; append chunks are not deduplicated unless the protocol supplies a real replay cursor or sequence.
- Publication uses only the committed receipt. A scope revision gap still triggers one replacement baseline under ADR-0023.
- Checkpoint or projection-rebuild failure is visible and recoverable; it never fabricates a complete Tool, idle Task, or lost terminal suffix.

## Staged migration

### Stage 0: lock in the regression and measurements

Add a fixture that emits 10,000 terminal-output deltas and about 400 KiB of output against a Task with at least 4 MiB of Chat. Record current file reads/writes, bytes serialized, fsync count, global-lock wait, Task revisions, Navigation events, queue depth, cancellation latency, and unrelated-Task text latency. This is the acceptance baseline, not a microbenchmark only.

### Stage 1: install the semantic pipeline on the current store

Introduce the ACP Adapter, typed semantic deltas, normalized no-op detection, bounded batching, barriers, fair per-Task scheduling, and independent cancellation control. Add a fixture-backed Adapter for the observed `terminal_output_delta` extension if OpenAIDE intends to support it. Stop treating every raw Tool update as an unconditional Tool, Task-summary, and Navigation mutation. This immediately prevents unbounded in-memory backlog and revision spam while making unsupported extension data visible in diagnostics instead of pretending it reached durable Tool detail.

If SQLite cannot follow immediately, extend the current journal only enough to support `UpsertMessage`/Tool summary replacement and append-only detail/terminal segments with a Task-local identity index. Keep the global file mutation lock while it remains necessary for rollback, but make each batch a small append rather than a 4.4 MiB rewrite. Do not introduce a second general-purpose file transaction framework.

### Stage 2: implement the SQLite Task State Store Adapter

Add the schema, dedicated storage executor, direct SQL transactions, projection reducer, rebuild checks, WAL configuration, and store-Interface conformance tests. Migrate one complete Task mutation path at a time behind the same semantic Interface; do not dual-publish or let workflow callers choose file versus SQL behavior.

### Stage 3: cut over state roots once

Because accepted OpenAIDE policy gives no compatibility guarantee for superseded development-only persisted state, prefer a one-time, verified importer only if preserving existing local development Tasks is valuable. Import task metadata, materialized Chat, Tool details, and validity clocks in one controlled migration, verify counts and identities, then make SQLite authoritative. Do not maintain indefinite dual-read, dual-write, or fallback deserialization.

### Stage 4: remove the file store and add the ACP v2 Adapter

Delete the materialized JSON mutation and rollback paths after conformance, recovery, and performance tests pass. An ACP v2 Adapter then maps its patch/replace/chunk operations directly to the existing semantic delta algebra; storage and Frontend contracts do not change.

## Observability

Measure the pipeline and transaction separately:

- raw updates, semantic deltas, reduced deltas, coalescing ratio, and bytes by kind;
- pending item/byte count, oldest queue age, backpressure duration, and per-Task fairness delay;
- batch reason, operation count, payload bytes, and source-ordinal range;
- storage queue wait, SQL transaction time, WAL/fsync time where measurable, projection time, and publication time;
- revisions and Navigation events per second by Task;
- Tool-detail bytes and terminal segment count;
- checkpoint duration, WAL size, rebuild count/duration, busy retries, and storage failures;
- cancellation request latency, `stopping` commit latency, prompt-response barrier latency, and time to idle;
- unrelated-Task p50/p95/p99 commit latency during a noisy stream.

Logs should carry Task id, Native Session id hash/token, event kind, counts, sizes, and durations, but not terminal output, prompts, raw ACP payloads, paths, or Tool detail content.

## Regression and conformance tests

Test through the two Module Interfaces, not internal SQL helpers:

1. **10,000-delta incident:** exact output survives restart; commit/revision count is bounded by batch policy; no full-history rewrite occurs; Navigation events remain bounded; unrelated text commits stay within the agreed latency budget.
2. **Ordering:** interleaved Agent text, Thought text, Tool patches, terminal appends, replacements, and status changes rebuild to exactly the live materialized state.
3. **Patch semantics:** omitted, clear, and set remain distinct; replacement followed by append and append followed by replacement match ACP semantics.
4. **Barrier:** prompt completion cannot publish idle before every earlier accepted update commits; later updates remain accepted through the session-lifetime consumer.
5. **Cancellation under flood:** `task/cancel` sends promptly, `stopping` publishes promptly, queued output remains durable, and the one terminal cancellation result is persisted.
6. **No subscribers:** Tool detail and terminal bytes persist completely and appear when a later detail subscription opens.
7. **Atomicity:** injected failures after event insert, projection update, Task update, and before commit publish nothing and recover to the prior revision.
8. **Restart/rebuild:** deleting or staling only materialized projections rebuilds the same Chat, Tool detail, clocks, and baseline from normalized events.
9. **Revision recovery:** one committed batch yields one contiguous Task revision; a missed event causes exactly one authoritative baseline install.
10. **Fairness:** multiple noisy Tasks cannot starve an interactive Task or metadata/cancel command.
11. **Backpressure:** bounded queues never exceed configured bytes/items, never lose a trailing state, and expose pressure rather than allocating without limit.
12. **SQLite concurrency:** multiple readers observe complete committed snapshots while short writers serialize; busy timeout and checkpoint behavior are deterministic under long-read fault injection.
13. **Ignored extension no-op:** an unknown `_meta` update that changes no normalized product state produces no storage write, revision, Task event, or Navigation event, while diagnostics record the unsupported update kind without content.
14. **Durability failure:** injected WAL sync/commit failure returns an error, publishes nothing, and leaves the previous authoritative baseline intact.

## Open questions requiring explicit decisions

1. **Power-loss durability:** this note interprets “durable before publish” as requiring SQLite WAL `synchronous=FULL`. If process-crash durability but not sudden-power-loss durability is sufficient, NORMAL is faster but changes the guarantee.
2. **Terminal retention:** the accepted specification requires Tool details without subscribers but does not define maximum terminal bytes, truncation direction, or event compaction. The pipeline must preserve everything until a product retention policy is agreed.
3. **v1 terminal extensions:** the incident's `terminal_output_delta` is implementation-specific ACP v1 metadata. The Adapter needs fixtures from each supported Agent to prove whether an update is an append, a cumulative replacement, or both.
4. **Initial batch thresholds:** 32/50 ms, 64 KiB, and 256 operations are starting points. The incident regression on slow and fast local filesystems must set final defaults.
5. **Importer scope:** accepted policy permits dropping superseded development state. Decide whether a one-time JSON-to-SQLite importer is worth its code and test burden before implementation.

The key architectural choice is not the exact threshold or table encoding. It is to place one semantic seam before persistence and one deep Task State Store Interface after it. That creates Locality for ordering, durability, replay, projection, and performance policy, while giving ACP v1, ACP v2, file-bridge, SQLite, and tests narrow Adapters instead of spreading protocol and storage mechanics across Task workflows.
