# P37 Prompt Content Test Layout API Contract

Completed: 2026-06-27T03:16:07+03:00

## Accepted Shape

Move the inline `#[cfg(test)]` module from:

- `agent/prompt_content.rs`

into:

- `agent/prompt_content/tests.rs`

Keep `prompt_content.rs` as the production implementation plus a `#[cfg(test)]`
external test-module declaration.

## Stable API

No caller-facing API changes:

- `PromptContentCapabilities`
- `PromptContentPolicy`
- `PromptContentError`
- `build_prompt_content_with_policy`
- `validate_prompt_attachments`

The test-only `build_prompt_content` helper may remain available under `#[cfg(test)]`
if existing tests need it.

## Ownership

- `prompt_content.rs` owns prompt text/attachment conversion and validation.
- `prompt_content/tests.rs` owns prompt-content unit tests and test-only helpers.

## Non-Goals

- No ACP prompt content behavior change.
- No attachment URI/path encoding behavior change.
- No Agent prompt capability mapping change.
- No new production abstraction.
- No test deletion or weakening.

## Review And Test Requirements

- Existing prompt-content tests must keep passing.
- `cargo test -p openaide-runtime agent::prompt_content::tests -- --nocapture` must pass.
- `cargo test -p openaide-runtime agent::acp::tests -- --nocapture` must pass because ACP prompt requests depend on this module.
- `cargo test -p openaide-runtime` and `npm test` must pass.
- Production `prompt_content.rs` should drop below the source-size limit once inline tests move out.
