# Pre-journal Task fixture

This synthetic Task uses the actual file layout and serialization written at
`ba9255e8ec4a691025ee3879be0346613285b513`, immediately before the July 21, 2026
journal cutover (`7a5fe7e8`). It was assembled from that revision's `TaskRecord`,
`message_store.rs`, `message_store/journal.rs`, and `ToolDetailArtifact` writers;
it is deliberately not serialized with today's model.

The fixture covers the old `visible` lifecycle plus independent Archive flag,
single User-owned title, a file attachment, a materialized journal checkpoint,
already-materialized and pending Agent text deltas, an appended Agent message,
and lazy Tool detail. Identifiers, content, and workspace paths are synthetic.

`message_meta.stale.json` represents process death after the fourth message was
synced but before the separate metadata replacement. The historical reader still
replayed all four messages; migration must reconstruct the count and cursors while
preserving the stored clock and the original bytes.
