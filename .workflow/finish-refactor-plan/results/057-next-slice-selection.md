# P36 Next Slice Selection

Completed: 2026-06-27T03:16:07+03:00

## Selected Slice

Move `agent/prompt_content.rs` inline tests into a separate Rust test module.

## Rationale

`agent/prompt_content.rs` appears oversized in the source-size scan, but much of that
size is inline test code. The production prompt-content boundary is already cohesive:
it owns conversion from OpenAIDE prompt text and attachments into ACP `ContentBlock`
values plus prompt attachment validation under Agent prompt capabilities.

Before changing prompt-content behavior or splitting production logic, move tests out
to satisfy the project rule that Rust tests should live in separate files where
practical. This makes later source-size scans reflect production code more accurately.

## Non-Selection

Do not change prompt attachment semantics in this slice.

Do not split URI/path encoding helpers yet. After the test-layout move, reassess the
remaining production size and cohesion before introducing another prompt-content
module.
