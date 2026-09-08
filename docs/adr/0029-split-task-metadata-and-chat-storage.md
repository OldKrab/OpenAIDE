# Split Task Metadata And Chat Storage

Status: accepted

This ADR supersedes ADR-0028's rule that one Task journal owns complete durable
Task state. Durable Task Metadata and Chat have separate authorities so live
Agent catalogs and repeated Task snapshots cannot dominate Chat storage.

An atomically replaced `task.json` owns Durable Task Metadata and points to the
current Chat snapshot and delta generations. Durable metadata includes title, Archive
and tombstone state, Project Context, Agent and Native Session binding, and
explicit user preferences. A `chat.snapshot.<generation>` file owns the materialized
Chat projection and its `chat.journal.<generation>` file owns normalized Chat deltas
and Tool-artifact visibility references accepted since that snapshot. The initial
generation uses the unsuffixed names. Generation pointers distinguish a committed
delta ahead of metadata from an obsolete pre-compaction tail left by a crash.
Configuration Option catalogs and preference-application progress are process-owned and
excluded from durable Task writes. Hydration discards legacy persisted option catalogs;
the live Native Session supplies fresh controls. The separate per-Agent preference store
contains only confirmed identifiers and values for initializing new sessions. Agent command
catalogs may persist as last-known data; their live freshness and pending mutations are
process-owned. Persisted data never establishes an active Agent attachment after restart.

Recovery follows one-way authority instead of cross-file transactions. Artifact
content is synced before its Chat reference. A Chat-changing transaction commits
its Chat operations and changed durable Task fields in one checksummed journal
frame before replacing `task.json`. This includes first-Send promotion, queue
consumption, Composer History, and Task revisions; recovery must never expose an
accepted User message while retaining its Prepared lifecycle or pending queue
item. Unchanged Task fields, including queued attachments and Agent catalogs,
are not repeated in ordinary Chat delta frames. A crash before the Chat reference
leaves an invisible artifact; a crash after the frame but before `task.json`
rolls forward both Chat and those Task facts. Process-owned active state is
recovered as interrupted or inactive rather than restored as live. Independent
metadata facts are changed by atomic file replacement.

Task metadata records the committed Chat journal byte length. Startup checks this
small checkpoint and replays only Tasks with additional journal bytes before
publishing Navigation or running Prepared-Task and queue recovery. A complete
newer frame rolls forward; an incomplete final frame is discarded. Frames already
covered by the metadata checkpoint cannot override later title, Archive, or queue
edits from `task.json`. Existing snapshots and journals remain readable: the byte
checkpoint and changed-field recovery payload are additive optional fields, and
files without a byte checkpoint retain their existing lazy hydration behavior.
Their first hydration durably establishes the checkpoint before any new Chat
frame can commit, so the upgrade itself cannot reopen the crash window.

Existing ADR-0028 journals migrate lazily on first Task access. Migration replays
the old journal once, writes and validates all replacement files, publishes them
atomically, and removes the old journal only after the new store is authoritative.
Startup and Task Navigation do not migrate or replay unopened Chat. A failed or
interrupted migration leaves the old journal authoritative and retryable.

The earlier file-backed `<state-root>/tasks` format is imported once at startup
through the current Task durability boundary. Its Task record, materialized Chat
checkpoint, complete append-journal records, Chat metadata, and Tool artifacts are
read and verified before the original directory moves into the current Task's
`legacy-files` backup. This preserves exact previous bytes while letting Task
deletion, retention, and Reset task history own the backup too. A restart between
the import commit and source move completes the move only when the destination
still matches the imported Task and artifacts. A conflicting destination,
malformed source, or unsupported shape remains intact and fails startup explicitly;
no existing history is overwritten or treated as an empty store.

## Schema Evolution

Durable Task files use versioned storage envelopes and adapters so incompatible
bytes are not deserialized directly into evolving runtime models. A loader reads
and validates the small versioned envelope first, adapts that version's durable
representation, and then decodes the current Task model. It migrates one Task
lazily at its normal hydration boundary. Migration writes and validates the
replacement before publishing it; failure leaves the prior bytes authoritative
and retryable.

Every incompatible persisted-shape change bumps the owning storage version and
ships the prior-to-current converter in the same change. A checked-in fixture
written in the prior released format must prove that the migration preserves the
relevant Task and Chat facts and that the current writer emits only the new
format. Migration steps remain composable so a current build can open Task
history from any earlier public release, including skipped beta releases. The
first migration converts the released Tool presentation shape
`{kind, subjects}` into the ordered `{actions}` representation.

Storage compatibility is forward-only. An older App Server must reject a newer
unsupported storage version without modifying its bytes; reinstalling a
compatible newer build restores access. OpenAIDE does not dual-write old formats
or retain downgrade copies.

At safe prompt or idle boundaries, committed Chat deltas are merged into a new
validated snapshot and the delta journal is reset. Cold Task opening therefore
loads one materialized snapshot plus a bounded delta tail rather than replaying
the lifetime history of Task and runtime-control changes.

Compaction is lossless and threshold-based. App Server checks it after completed
turns, after authoritative Native Session history replacement, and during startup
and six-hour idle maintenance. A journal is rewritten after 128 superseded frames
or when the physical rewrite can reclaim at least 1 MiB at a material ratio.
Repeated byte-identical Tool-detail replacements do not create another artifact
revision. Artifact compaction keeps the committed frame sequence and every
terminal append while retaining only the latest structured replacement, so
existing artifact heads and later appends remain valid.

Client-open recency is a small Task-local `last-used` marker rather than Durable
Task Metadata. It can therefore protect seven-day local retention without
appending a full Task snapshot on every open. Invalid usage markers fail closed.
Tasks carried forward from a store version without usage markers receive one
tracked seven-day grace period because their historical client opens are unknown.
Retention first publishes a local Task tombstone, then the single journal worker
renames and removes only that Task directory behind an exclusive durability
barrier. Agent-owned Native Sessions and Worktrees are outside this operation.
