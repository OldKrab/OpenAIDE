# Next Slice Selection: Task Turn Event Sink Split

## Decision

Select the Task Turn event sink split as the next Backend refactor slice.

## Why This Slice

`tasks/turn_events.rs` currently owns several separate responsibilities:

- live `AgentEventSink` and `AgentSessionEventSink` adapter implementations;
- streamed text and thought coalescing;
- durable Task/Chat message commits for Agent events;
- Task config option update commits;
- permission request waiter registration and blocking wait behavior.

Those responsibilities are related, but they have different invariants and failure
modes. Splitting them now reduces the risk of future Task recovery, permission
broker, and App Server Protocol event work because Agent output ingestion becomes
easier to review in small boundaries.

## Proposed Boundary

Keep `tasks/turn_events.rs` as the public task event-sink facade for:

- `TaskEventSink`;
- `TaskSessionEventSink`;
- `PermissionWaiters`;
- imports used by current Task lifecycle code.

Move focused internals into child modules:

- streaming run accumulation for text and thought chunks;
- permission waiter state and cancellation-aware waiting;
- Task config option update commit helper.

The facade should keep event routing and mutation calls readable, while child
modules own state mechanics that do not need to be visible to Task lifecycle
callers.

## Out Of Scope

- No behavior changes to event normalization, message ids, unread flags,
  revisions, status updates, or config option projection.
- No migration to `server_requests` broker in this slice.
- No App Server Protocol event publication rewrite.
- No Agent ACP I/O changes.
- No transport dispatch changes.

## Next Step

Grill and record the accepted API contract for this slice before implementation.
