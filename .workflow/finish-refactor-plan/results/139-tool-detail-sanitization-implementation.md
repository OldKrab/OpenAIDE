# Tool Details Sanitization Split: Implementation

## Implemented

Created `agent/tool_details_sanitizer.rs` for display safety and preview policy:

- preview truncation;
- sensitive-key classification;
- command summary sanitization;
- command-array normalization;
- scalar field summary sanitization;
- path leaf summaries;
- path-like detection;
- per-field summary classification.

Kept `agent/tool_details_io.rs` as the raw ACP tool input/output projection
facade:

- raw JSON object parsing;
- top-level input/output field selection;
- excluded-field handling;
- extra scalar field sorting and limiting;
- `ActivityToolInput`, `ActivityToolOutput`, and `ActivityToolField`
  construction.

`agent/tool_details.rs` continues to import the same helper surface through
`tool_details_io.rs`; no caller learns about the sanitizer module.

## Preserved Behavior

- Redaction, path leaf summaries, command summaries, scalar summaries, and
  preview truncation are unchanged.
- `ActivityTool*` protocol shapes are unchanged.
- Field ordering and extra scalar field limits are unchanged.
- ACP tool-call event projection remains unchanged.

## Verification

Passed:

- `cargo fmt --all --check`
- `cargo test -p openaide-runtime tool_call_preview_does_not_expose_raw_fields_or_full_diff_paths -- --nocapture`
- `cargo test -p openaide-runtime`

## Next Step

Run `$doomsday-review` on this slice and fix material findings before final
integration verification.
