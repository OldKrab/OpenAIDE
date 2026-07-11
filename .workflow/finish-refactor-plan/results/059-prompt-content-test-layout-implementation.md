# P38 Prompt Content Test Layout Implementation

Completed: 2026-06-27T03:20:22+03:00

## Implemented

- Moved the inline prompt-content unit tests from `agent/prompt_content.rs` to
  `agent/prompt_content/tests.rs`.
- Left `agent/prompt_content.rs` with production prompt-content conversion code plus
  a `#[cfg(test)] mod tests;` declaration.
- Kept the test-only `build_prompt_content` helper under `#[cfg(test)]`.

## Behavior

No production prompt-content behavior changed. Attachment URI/path encoding, prompt
capability handling, embedded payload conversion, and validation behavior are unchanged.
