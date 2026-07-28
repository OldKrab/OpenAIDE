# Keep Composer History as a bounded durable projection

Composer History is stored as an App Server-owned bounded projection of text accepted through OpenAIDE rather than derived from Chat. Native Session history replacement can change User message identities and does not provide original per-message timestamps, so Chat cannot supply trustworthy OpenAIDE-send provenance or exact cross-Task recency; duplicating a bounded amount of accepted text preserves deterministic Task- and Project-scoped recall while attachments and imported Native Session messages remain outside the projection.

## Consequences

The App Server owns acceptance order, exact-text deduplication, scope, and retention, while Frontend owns only ephemeral keyboard browsing state. Archive and Task deletion do not proactively purge Project recall entries, and the projection must remain bounded while supplying the 50 newest unique values for a Task or selected Project.
