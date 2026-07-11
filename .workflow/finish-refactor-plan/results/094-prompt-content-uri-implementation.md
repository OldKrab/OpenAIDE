# Prompt Content URI Split Implementation

## Scope

Implemented the accepted contract in `093-prompt-content-uri-api-contract.md`.

## Code Changes

- Added `agent/prompt_content_uri.rs` for pure prompt attachment resource identity
  helpers.
- Moved attachment resource naming, file URI normalization, embedded attachment URI
  generation, URI-scheme detection, platform path detection, and percent encoding
  out of `agent/prompt_content.rs`.
- Kept prompt block construction, payload classification, capability decisions,
  fallback behavior, validation, and error construction in `agent/prompt_content.rs`.
- Registered the new internal module in `agent/mod.rs`.

## Stable Behavior

- No public Agent runtime API changed.
- No protocol `Attachment` shape changed.
- No ACP `ContentBlock` shape changed.
- No user-facing error text changed intentionally.
- No URI normalization, percent encoding, fallback, or path handling behavior changed
  intentionally.

## Source Size

- `agent/prompt_content.rs`: 257 lines, down from 398.
- `agent/prompt_content_uri.rs`: 145 lines.
- `agent/mod.rs`: 279 lines.

All touched production files are below the 400-line limit.
