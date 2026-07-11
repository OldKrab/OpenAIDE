# Prompt Content URI Split Integration Verification

## Focused Checks

- `cargo fmt --all`
- `cargo test -p openaide-runtime agent::prompt_content::tests -- --nocapture`
- `cargo test -p openaide-runtime agent::acp::tests::prompt_content_includes_text_and_resource_links_for_path_attachments -- --nocapture`
- `cargo check -p openaide-runtime`
- `git diff --check`
- Source-size scan for touched production files

## Full Checks

- `cargo test -p openaide-runtime`
- `npm run check`
- `npm test`

## Source Size

- `openaide-rs/app-server/src/agent/prompt_content.rs`: 257 lines
- `openaide-rs/app-server/src/agent/prompt_content_uri.rs`: 145 lines
- `openaide-rs/app-server/src/agent/mod.rs`: 279 lines

All touched production files are below the 400-line limit.

## Result

All checks passed.
