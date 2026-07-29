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
delta ahead of metadata from an obsolete pre-compaction tail left by a crash. Agent command and Configuration Option
catalogs, pending requests, and active runtime controls are Transient Task Runtime
State and are never durable.

Recovery follows one-way authority instead of cross-file transactions. Artifact
content is synced before its Chat reference. Chat is committed before updating
derived Task metadata. A crash before the Chat reference leaves an invisible
artifact; a crash after Chat but before `task.json` leaves metadata that can be
repaired from Chat. Process-owned active state is recovered as interrupted or
inactive rather than restored as live. Independent metadata facts are changed by
atomic file replacement.

Existing ADR-0028 journals migrate lazily on first Task access. Migration replays
the old journal once, writes and validates all replacement files, publishes them
atomically, and removes the old journal only after the new store is authoritative.
Startup and Task Navigation do not migrate or replay unopened Chat. A failed or
interrupted migration leaves the old journal authoritative and retryable.

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
