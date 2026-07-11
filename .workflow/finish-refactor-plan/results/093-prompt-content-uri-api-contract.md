# P72 Prompt Content URI API Contract

Completed: 2026-06-27T04:37:25+03:00

## Accepted Shape

Add `agent/prompt_content_uri.rs` as a focused internal module.

`prompt_content_uri.rs` owns pure attachment resource identity helpers:

- `attachment_resource_name(&Attachment) -> String`
- `attachment_resource_uri(&Attachment) -> Option<String>`
- `embedded_attachment_uri(&Attachment) -> String`
- internal `file_path_uri`
- internal `file_uri`
- internal `has_uri_scheme`
- internal `is_windows_absolute_path`
- internal `percent_encode_path`
- internal `percent_encode_uri_path`

`agent/prompt_content.rs` keeps public prompt-content behavior:

- `PromptContentCapabilities`
- `PromptContentPolicy`
- `PromptContentError`
- `build_prompt_content_with_policy`
- test-only `build_prompt_content`
- `validate_prompt_attachments`
- prompt payload classification and ACP `ContentBlock` construction
- error construction using the helper-provided attachment resource name

## Stable API

- No public Agent runtime API changes.
- No call-site import churn outside `agent/prompt_content.rs` and `agent/mod.rs`.
- No protocol `Attachment` shape changes.
- No user-facing error text changes.
- No ACP `ContentBlock` shape changes.
- No URI normalization, percent encoding, fallback, or path handling behavior changes.

## Ownership

- `prompt_content_uri.rs` owns how an `Attachment` gets a safe resource name or URI.
- `prompt_content.rs` owns when a name/URI is used in prompt block construction.
- Neither module owns attachment storage, allowed-root validation, shell file picking,
  or Agent prompt execution.

## Non-Goals

- No attachment feature redesign.
- No support for new URI schemes.
- No change to embedded resource URI format.
- No change to file URI host handling.
- No change to prompt image/audio/embedded-context capability logic.
- No test deletion or weakening.

## Review And Test Requirements

- Existing prompt-content tests must keep passing.
- Existing ACP prompt attachment tests must keep passing.
- `cargo test -p openaide-runtime agent::prompt_content::tests -- --nocapture`
  must pass.
- `cargo test -p openaide-runtime agent::acp::tests::prompt_content_includes_text_and_resource_links_for_path_attachments -- --nocapture`
  must pass.
- `cargo test -p openaide-runtime`, `npm run check`, and `npm test` must pass
  before commit.
- All touched production source files must remain below the 400-line limit.
