# P71 Next Slice Selection

Completed: 2026-06-27T04:37:25+03:00

## Selected Slice

Split prompt attachment URI and resource naming helpers out of
`agent/prompt_content.rs`.

## Rationale

`agent/prompt_content.rs` is 398 lines, just under the production source-file limit.
It mixes two responsibilities:

- prompt content block construction and capability-driven payload selection;
- attachment display/resource naming, file URI normalization, embedded attachment
  URI generation, URI-scheme detection, platform path detection, and percent
  encoding.

The URI/resource identity helpers form a cohesive lower-level module and are already
pure. Extracting them reduces the near-limit file and makes the prompt construction
module easier to review without changing Agent prompt behavior.

## Non-Selection

Do not change prompt capability decisions, fallback behavior, ACP content block
selection, attachment validation behavior, or user-facing error text.

Do not move ACP prompt execution, session worker policy construction, attachment
storage/runtime, or protocol `Attachment` shape in this slice.
