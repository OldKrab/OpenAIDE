# Task Journal Storage

Status: accepted

This ADR supersedes the Task-storage implementation in
[ADR-0024](0024-task-chat-persistence.md) and the SQLite requirement for the
Task persistence slice in
[ADR-0022](0022-backend-frontend-app-shell-architecture.md). It preserves the
Task, Chat, Tool-detail, and publication behavior accepted by
[the Task Lifecycle and Chat specification](../task-chat-flow.md), except for
the revision clarification below.

## Scope And Ownership

The replacement owns complete durable Task state: Task records, normalized
Chat, lifecycle state, Task-local ordering, and lazy Tool-detail artifacts.
Projects, Agents, settings, worktree inventory, diagnostics, and other
state-root stores remain unchanged.

Exactly one App Server process owns a state root. App Shell clients attach to
that process instead of opening Task storage independently. Inside the process,
one fair storage worker serializes physical writes while maintaining bounded
per-Task queues. A noisy Task receives at most one batch per scheduling round.
A reserved control lane prevents Stop, permission resolution, prompt
completion, and shutdown barriers from being starved by streamed output.
The worker drafts only the affected Task and does not hold the root projection
lock during file I/O; unrelated reads and commit cost do not scale with every
stored Chat history.

Existing file-backed Tasks are not migrated. Their Native Sessions appear as
unadopted sessions and may be adopted into fresh Tasks. Legacy bytes are not
silently deleted.

## Normalized Operations And Admission

ACP updates pass through a strict semantic adapter. Recognized input becomes a
small typed operation such as message append, text append, Tool patch, terminal
append, or lifecycle transition. The observed
`_meta.terminal_output_delta` extension is a supported terminal append input.
Unknown shapes are diagnosed without persisting arbitrary ACP envelopes. An
update that changes no normalized product state creates no journal record,
revision, or publication.

Callers submit normalized operations through bounded admission and receive a
commit receipt. Streaming callers do not await each receipt. Durability
barriers await their receipt and seal every earlier operation for that Task.
Dropping a receipt never cancels admitted work.

The initial batching bounds are 32 milliseconds, 64 KiB, or 256 operations,
whichever is reached first. They are internal defaults calibrated by the
storage benchmark, not user settings. Queue capacity is bounded globally and
per Task. Full queues apply backpressure to data updates while reserving
capacity for control barriers; durable output is never silently dropped.

## Task Journal

Each Task has one canonical, versioned journal. A file header identifies the
format. Every commit is one length-delimited frame containing a typed JSON
payload, consecutive Task-local journal sequence, and checksum. The sequence is
storage-private and exists for replay, barriers, compaction, and integrity
validation.

A complete final frame with an invalid checksum, a sequence gap, malformed
payload, or unsupported format fails replay visibly and leaves original bytes
untouched. Only an incomplete final frame is discarded automatically after a
crash. Replay, live mutation, compaction, and tests use the same normalized Task
model.

At an idle prompt boundary, the worker may compact a Task journal whose obsolete
record count or byte ratio crosses a measured threshold. Compaction writes and
syncs a unique sibling temporary journal, validates it through normal replay,
publishes it with a durable platform-aware replacement, and resumes appends only
after publication succeeds. The current atomic JSON helper is not sufficient:
sync failures, unique temporary-file ownership, parent-directory durability,
and cross-platform replacement are required parts of this module.

The initial measured policy compacts after 128 obsolete frames, or when the
journal is at least twice the canonical projection size and would reclaim at
least 64 KiB. A forced compaction remains available to verification and
maintenance code; ordinary prompt completion only requests the measured check.

## Lazy Tool Artifacts

Full Tool details remain per-tool-call lazy artifacts as required by
[ADR-0020](0020-tool-detail-artifacts.md). Progressive artifact operations are
typed and framed. Terminal-like output appends combined chunks instead of
replacing the complete output.

An artifact change is prepared and synced before the Task journal appends a
small visibility reference. The synced Task reference commits visibility. A
crash before that reference leaves an invisible artifact tail that recovery
ignores and truncates. A durable reference always points to bytes synced first.
There is no rollback and no independent partially visible update.

Task replay records committed artifact heads. Tool-detail replay exposes only
frames at or below those heads. A missing or corrupt artifact makes that Tool
detail unavailable without making the Task snapshot unreadable.

Startup validates artifact journals with a streaming scan that retains frame
offsets rather than lifetime output payloads. Normal appends trust the
startup-reconciled head and append directly; an uncertain prepare or Task-head
commit freezes that Task instead of replaying or retrying. Reconciliation skips
unavailable or quarantined Tasks because no authoritative head exists and their
original artifact bytes are diagnostic evidence.

The first implementation does not rewrite large artifact journals. It may append
structured-detail checkpoints and index frame offsets for lazy reads. Automatic
artifact compaction is deferred until measurements show enough reclaimable
redundancy to justify another recovery path. Accepted output is not truncated;
the first implementation has bounded memory and frame sizes but no lifetime disk
quota.

## Durability, Failure, And Cancellation

Every batch syncs before publication. Artifact-bearing batches sync prepared
artifact bytes first and the Task visibility reference second. Send, Stop,
permission and question resolution, Task status transitions, prompt completion,
session replacement, compaction, and shutdown are durability barriers.

When append or sync durability cannot be confirmed, the batch is not published.
The affected Task freezes, its active Agent turn is cancelled, its bytes remain
available for diagnosis, and unrelated Tasks continue. Storage-worker failure
that invalidates durability for the complete state root stops the App Server.
The runtime never retries an uncertain commit or presents memory-only output as
durable.

Stop preserves the accepted cancellation behavior. It seals all earlier
same-Task updates, durably changes status to `stopping`, then publishes and sends
ACP cancellation. Late session updates remain ordered and inspectable until the
prompt settles, but cannot revive cancelled work or turn interrupted Tools into
successful completion.

## Revisions And Publication

Journal sequence, Task revision, and subscription cursor have separate owners:

- journal sequence orders durable storage frames and is never public;
- Task revision advances once when a durable batch changes `TaskSnapshot`;
- each existing subscription scope uses its own event cursor for delivery and
  reconnect recovery.

Terminal-only artifact commits do not change Task revision. They publish append
data through the existing Tool-detail subscription scope. A batch that also
changes the lightweight Tool row advances Task revision once and publishes both
affected scopes after durability. Task Navigation changes only when the
projected `TaskSummary` value changes; internal history counters never invalidate
Navigation by themselves.

A mixed structured-plus-terminal artifact commit publishes one atomic delta at
that artifact revision: structured replacement preserves the replica's earlier
terminal state, then same-frame terminal appends apply exactly once. Initial
subscription still returns a complete baseline. A Tool-detail delta may advance
only to the exact next artifact revision; a gap invalidates the replica and
requires a fresh baseline.

This clarifies ADR-0023's phrase "each durable Task transaction": a durable
change to the Task snapshot advances Task revision, while a Tool-detail-only
artifact commit is ordered by its Tool-detail subscription cursor.

## Verification Gate

The replacement does not cut over until all of the following pass:

- deterministic replay matches an independent reference model;
- restart fault injection covers every write, sync, artifact-reference, and
  compaction publication boundary;
- incomplete-tail recovery and visible non-tail corruption behavior are proven;
- bounded queues, backpressure, fair scheduling, control barriers, shutdown,
  and per-Task failure isolation are proven;
- the recorded 10,002-update terminal workload persists every accepted byte
  exactly once without Task or Navigation invalidation for output-only batches;
- identical semantic workloads compare current and replacement wall time,
  p50/p95/max latency, Stop latency, bytes written, sync count, peak queued
  bytes, replay time, compaction time, and event counts at multiple Task sizes.

The full replacement workload is measured. The current rewrite-heavy store may
be measured at bounded sample sizes and reported with its observed scaling when
running the complete case would generate excessive writes.
