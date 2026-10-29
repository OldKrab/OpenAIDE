# Subagent Histories Are Task Journal Chat Streams

Status: accepted

This ADR amends ADR-0046. A Subagent History is the same kind of data as the Main
Agent's Chat: normalized Chat from one ACP session. ADR-0046 nevertheless gave it a
storage engine of its own, which writes synchronously on the ACP connection thread,
reloads the whole history and syncs three files for every streamed chunk, and has
needed its own copies of frames, compaction, an upgrade sweep, and live deltas. The
Task journal (ADR-0028, ADR-0029) already does all of this for the Main Agent's Chat.

The Task journal worker therefore writes every Chat of a Task: the Main Agent's Chat
and one Chat stream per Subagent, addressed by Subagent ID. Each stream keeps its own
snapshot and journal generation files under the Task, so ADR-0046's independence
holds: a Task opens without reading its Subagent Histories, a history loads only
when a client selects it, and a damaged stream makes only that history unavailable.

Subagent writes use the Main Chat's operations and commit boundaries. Streamed child
output is a stream write the ACP thread does not wait for. Spawn, lifecycle, identity,
and permission outcomes are barriers, so their callers keep receiving an error. All
writes of a Task share one ordered queue, which preserves ADR-0046's ordering: spawn
before child output, terminal lifecycle after it. The worker keeps open streams in
memory, batches them with one sync per file per batch, applies ADR-0029's compaction
thresholds, and hands each committed Subagent revision to the same commit handler
that publishes `subagentHistoryChanged`.

A failed physical write of a Subagent stream freezes that stream and marks its history
unavailable; unlike a Main Chat failure, it does not freeze the Task. The Subagent
catalog (summary tree and lifecycle) is written only when a summary or lifecycle
changes, never per chunk; the current history revision comes from the stream.

Existing Subagent histories migrate lazily under ADR-0029's schema rules: the first
access replays the ADR-0046 snapshot and journal once, publishes validated stream
files, and only then removes the old files. The Task's Subagent catalog moves to
schema 2 before its first history migrates, so an older App Server rejects that
catalog without modifying it instead of reporting migrated histories as lost. The ADR-0046 frame format, its compaction, and its
startup sweep are removed once migration is in place.

## Considered Options

- A writer thread per session running today's save path: removes the wait from the
  ACP thread but keeps the reload, the per-chunk syncs, and a second storage engine.
- A batched writer only for Subagent histories: fixes the cost by building a second
  copy of the Task journal, which every later Chat storage change would have to match.
