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

At safe prompt or idle boundaries, committed Chat deltas are merged into a new
validated snapshot and the delta journal is reset. Cold Task opening therefore
loads one materialized snapshot plus a bounded delta tail rather than replaying
the lifetime history of Task and runtime-control changes.
