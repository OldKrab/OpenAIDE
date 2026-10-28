# Task Subscription Resumes From a Known Revision

Status: accepted

A Frontend that returns to a Task it has already seen tells `state/subscribe` the Task revision of its cached copy. The App Server then replies that the copy is current, sends only the Task changes after that revision, or falls back to a complete baseline. Without this, every Task switch serializes, sends, and maps the whole Task snapshot, so switching cost grows with history size even when nothing changed. This amends [ADR-0023](0023-task-state-publication-and-replica-recovery.md): a Task subscription no longer always returns a complete baseline.

## Choice

- **Request.** For a Task scope, `state/subscribe` may carry a known revision. Frontend sends one only for a cached Task copy installed in the current replica epoch; a copy from an earlier App Server generation or replaced transport session is never offered. Other scopes keep their small baselines.
- **Reply.** The App Server answers with exactly one outcome:
  - **current**: the known revision is the Task's current revision, so no Task state is sent;
  - **missed changes**: the `taskChanged` events after the known revision, in revision order, exactly as they were published;
  - **baseline**: a complete Task baseline, as before, when no revision was offered, the revision is ahead of the Task, or the missed events are no longer retained.
- **Transient state.** Pending Permission and Question requests travel on the Task stream without a Task revision, so every reply also carries the Task's current pending requests. A cached copy never keeps request controls that closed while the user was away.
- **One ordering point.** The App Server chooses the outcome and registers the subscription at the same point in the Task's ordered stream where it takes a baseline today. The first live event after the reply is the next Task revision.
- **Applying missed changes.** Frontend applies them through the same ordered ingestion as live events. A gap or any failure to apply invalidates the copy and requests one baseline, the existing recovery path. No new merge logic is introduced.
- **Retention.** The App Server keeps recently published `taskChanged` events in memory, bounded in bytes per Task and in total, and discards the oldest first. Retention is a cache, not durable state: a restart discards it, and a miss costs one baseline. The bounds are chosen from measured event sizes.
- **Presence.** Unsubscribing on leave is unchanged. A Task the user left is no longer open in that client, so cleanup, archive-older, and request routing behave as today.

## Considered Options

- **Stay subscribed to recently visited Tasks.** It needs no protocol change, but every running Task the user is not viewing keeps streaming into the Frontend. A subscription also counts as the Task being open, which blocks cleanup and archive-older and makes the client a responder for that Task's server requests; avoiding that would need a second, background kind of subscription.
- **Record the last-changed revision on every stored Chat item.** Missed changes could then always be derived from storage, including across restarts. It changes the Task journal format, and after images become references a restart baseline is small, so it adds permanent storage work for little benefit.

## Consequences

- `task/open` still returns its own complete snapshot. The full saving needs `task/open` to stop carrying Task state, which is decided separately.
- The rule "every change to the Task snapshot advances the Task revision" becomes load-bearing: a Task field that changed without a revision would stay stale behind a **current** reply. Fields outside the revision, like pending requests, must be resent with every reply.
- Each subscribe logs its outcome, the baseline reason when one is sent, the known and current revisions, the number and bytes of missed events, and its duration, so a slow switch shows which path it took.
