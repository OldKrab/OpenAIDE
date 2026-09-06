This synthetic fixture preserves the full-history JSONL shape emitted by
`storage/subagents.rs` at `326d6ba2963d93913e4eb7b64f2da9a87df8a928`.
It was captured from the public Store writer before text deltas were added,
then reduced to one short message and stable test identifiers. It is deliberately
stored as bytes rather than serialized with the current journal model.
