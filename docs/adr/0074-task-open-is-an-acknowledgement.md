# Task Open Is an Acknowledgement

Status: accepted

`task/open` performs the open and returns only an acknowledgement. The Task subscription is the only source of the Task state Frontend shows. Before this change, every Task switch built, serialized, sent, and mapped a complete Task snapshot twice, once from `task/open` and once from the subscription baseline. That second copy also cancelled the saving from [ADR-0073](0073-task-subscription-resumes-from-a-known-revision.md). This amends [ADR-0023](0023-task-state-publication-and-replica-recovery.md): opening an existing Task no longer returns stored state.

## Choice

- **The open keeps its work.** `task/open` still checks access, records Task use, clears unread and attention, marks cached Native Session controls as recovering, and starts recovery in the background. Its errors are unchanged: a missing, removed, or inaccessible Task fails the request, and Frontend shows **Task not found** as before.
- **The result carries no Task state.** Success means the open committed. Everything the open changed, including cleared unread and recovering controls, reaches the client as ordinary Task revisions on the subscription.
- **Route readiness.** The Task view stays ready only when both the Task subscription and the open have been accepted. Frontend correlates the open with the requested Task id, never with a Task copy in the reply.
- **Other Task copies are unchanged.** Snapshot refresh after recovery and Prepared Task reuse still request their own copies; they are not part of Task switching.
- **Protocol version.** Removing the Task from the result is a change older clients cannot ignore, so the App Server Protocol major version increases, as [ADR-0062](0062-remote-devices-are-app-server-clients.md) requires. This shares the 3.0 bump with [ADR-0075](0075-chat-images-are-references.md).

## Considered Options

- **Keep the reply, and have Frontend ignore it when the subscription copy is current.** No protocol change, but the App Server still builds and sends the copy on every switch.
- **Make the subscription perform the open.** One request instead of two, but subscribing would gain side effects: a reconnect that only re-subscribes would also clear unread and record use.
- **An optional request flag that asks for no Task copy.** It avoids a major bump on its own, but keeps two result shapes until the next major. The major bump that ADR-0075 already needs makes the flag pointless.

## Consequences

- `task/markRead` and `task/reloadNativeSession` still return complete Task copies. They are outside Task switching and are not changed here.
- The open logs its outcome and duration as before. A route that never becomes ready shows which of the two acceptances is missing.
