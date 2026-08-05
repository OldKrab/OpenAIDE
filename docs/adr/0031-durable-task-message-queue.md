# Keep queued messages in durable Task state

OpenAIDE keeps each Task Message Queue as bounded App Server-owned Task state rather than Frontend drafts or Chat. Queue mutations and delivery share one durable per-Task ordering, and consuming an item atomically replaces queued state with a normal accepted User message before ACP I/O; this preserves recovery without replaying uncertain prompts while allowing automatic head delivery after `end_turn` and explicit manual use through ordinary Send or steering semantics.

## Consequences

The queue is published through the existing Task snapshot and revision stream, retains its own attachment resources, and pauses after every non-normal outcome or App Server restart. Edit in Composer atomically removes one observed item and transfers its content into the client-local ordinary draft; it deliberately does not retain a second guarded copy. This duplicates bounded message content outside Chat until delivery while preventing Frontend lifetime, navigation, reconnect, or process recovery from losing acknowledged follow-up work before the user explicitly extracts it.
