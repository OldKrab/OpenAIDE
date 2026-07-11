# Backend Prompt Content Split

## Contract

Split focused Agent prompt-content helpers out of
`openaide-rs/app-server/src/agent/prompt_content.rs` while preserving
`PromptContentCapabilities`, `PromptContentPolicy`, `PromptContentError`,
`build_prompt_content_with_policy`, and `validate_prompt_attachments` as the
stable Agent prompt-content API.

Ownership:

- `prompt_content.rs`: public facade, policy/error types, public builder and
  validator entry points, and test-only no-capability helper.
- `prompt_content/attachments.rs`: attachment routing and fallback from
  unsupported embedded file payloads to resource links.
- `prompt_content/payload.rs`: payload field extraction and image/audio
  attachment detection.
- `prompt_content/blocks.rs`: payload-to-ACP `ContentBlock` conversion for
  image, audio, text embedded resource, and blob embedded resource blocks.
- `prompt_content/resources.rs`: resource-link construction and attachment
  error formatting.

Do not change text block ordering, attachment order, fallback from unsupported
embedded file payloads to resource links, image/audio capability checks,
default image/audio MIME types, embedded-context capability checks, text/blob
embedded resource construction, synthetic embedded URIs, resource-link MIME
propagation, payload field aliases, error message text, public test helper
behavior, ACP schema mapping, prompt capability policy, attachment URI rules,
Agent runtime behavior, Task attachment semantics, or protocol/storage records
in this slice.

Focused tests:

- Prompt-content unit tests cover text, file links, embedded text, fallback,
  and unsupported attachment behavior.
- ACP prompt-content tests cover integration block construction.
- Runtime contract prompt attachment tests cover Agent delivery behavior.

## Implementation

Implemented the split by keeping `prompt_content.rs` as the public facade and
moving attachment routing, payload conversion, payload parsing, and resource
helpers into focused private modules.

Production source sizes after split:

- `prompt_content.rs`: 92 lines.
- `prompt_content/attachments.rs`: 37 lines.
- `prompt_content/blocks.rs`: 115 lines.
- `prompt_content/payload.rs`: 47 lines.
- `prompt_content/resources.rs`: 24 lines.

## Review

`$doomsday-review`:

- Correctness/spec/tests: no findings.
- Code quality: local pass found no findings.

## Verification

Focused checks already run:

- `cargo fmt --all --check`: pass after formatting.
- `cargo check -p openaide-runtime`: pass.
- `cargo test -p openaide-runtime agent::prompt_content::tests -- --nocapture`: pass.
- `cargo test -p openaide-runtime agent::prompt_content -- --nocapture`: pass.
- `cargo test -p openaide-runtime agent::acp::tests::prompt_content_includes_text_and_resource_links_for_path_attachments -- --nocapture`: pass.
- `cargo test -p openaide-runtime prompt_attachments_are_sent_to_agent_runtime -- --nocapture`: pass.
- `cargo test -p openaide-runtime prompt_content -- --nocapture`: pass.
- `cargo test -p openaide-runtime prompt_attachments -- --nocapture`: pass.

Final checks:

- `npm run check`: pass.
- `npm test`: pass after rerunning one unrelated flaky ACP active-session timeout.
- `git diff --check`: pass.
- `jq empty .workflow/finish-refactor-plan/state.json`: pass.
- Changed production source-size scan: largest split file is
  `prompt_content/blocks.rs` at 115 lines.

## Commit

This commit: `refactor: split backend prompt content`.

## Next

After this slice is committed, select the next compact refactor slice from the
current plan and architecture/file-size pressure.
