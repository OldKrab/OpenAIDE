# Task Chat Persistence

Status: accepted

This ADR defines durable Task lifecycle and Chat storage for the accepted [Task Lifecycle and Chat Specification](../task-chat-flow.md).

## Chat Projection And Append Journal

`messages.jsonl` is the materialized Chat projection. A per-Task append journal durably records a new Agent message or one text delta without reading, serializing, or replacing complete history. App Server keeps a Task-local identity index after first materialization so later chunks locate their row in constant time. Each journal record is synced before its `taskChanged` revision is published.

An ordinary Task transaction backs up materialized history with a same-filesystem hard link and records the append-journal length. Rollback truncates uncommitted journal records and invalidates the Task-local index; it does not copy complete Chat for each chunk.

At a primary prompt boundary, App Server materializes the journal once and removes it. The materialized file records the highest included journal sequence. A crash between materialization and journal removal therefore replays no delta twice, and later live chunks continue with newer sequences. Compaction failure is diagnostic: it does not revoke an already committed prompt result or detach the Native Session update consumer.

## Local History Clock

Each Task persists `localHistoryUpdatedAt`, the time when its stored Chat projection last changed. It advances when App Server accepts a User or steering message, persists Agent text or Chat activity, or replaces history after `session/load`. Opening or reading a Task, title changes, configuration and command changes, and unrelated Task metadata do not advance it.

History synchronization compares the cached Native Session timestamp only with this clock. The synchronization algorithm is defined by [ADR-0023](0023-task-state-publication-and-replica-recovery.md).

## Task Lifecycle

Persisted lifecycle state directly represents:

```text
TaskLifecycle
  New { ownerClientInstanceId }
  Visible
```

New Task lookup is transactionally unique by `clientInstanceId`. Agent and Project Context alone never identify a reusable New Task. Task title persistence follows the single title and provenance model defined by the specification; the Frontend-only `New task` fallback is never persisted and never determines lifecycle.
