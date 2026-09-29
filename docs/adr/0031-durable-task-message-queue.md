# Keep queued messages in durable Task state

OpenAIDE keeps each Task Message Queue as bounded App Server-owned Task state rather than Frontend drafts or Chat. Queue mutations and delivery share one durable per-Task ordering, and consuming an item atomically replaces queued state with a normal accepted User message before ACP I/O; this preserves recovery without replaying uncertain prompts while allowing automatic head delivery after `end_turn` and explicit manual use through ordinary Send or steering semantics.

## Consequences

Scheduled messages extend the same queue with an optional earliest delivery time in UTC. App Server owns timer eligibility and checks it under the existing per-Task acceptance gate; a due head starts only while idle and never steers active work. Queue order remains authoritative, including when a future head holds later items. Explicit Resume Queue rearms delivery without overriding the time; Send now overrides it. Scheduling does not change process liveness or restart recovery: App Server must be running, and restart pauses every pending schedule until explicit user recovery.

The queue is published through the existing Task snapshot and revision stream, retains its own attachment resources, and pauses after every non-normal outcome or App Server restart. Edit in Composer atomically removes one observed item and transfers its content into the client-local ordinary draft; it deliberately does not retain a second guarded copy. This duplicates bounded message content outside Chat until delivery while preventing Frontend lifetime, navigation, reconnect, or process recovery from losing acknowledged follow-up work before the user explicitly extracts it.
