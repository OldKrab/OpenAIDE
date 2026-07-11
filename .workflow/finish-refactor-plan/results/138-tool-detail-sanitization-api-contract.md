# Tool Details Sanitization Split: API Contract

## Accepted Contract

Split display sanitization and redaction policy out of
`agent/tool_details_io.rs` without changing tool-detail projection behavior.

## Caller-Facing Surface

`agent/tool_details_io.rs` remains the raw tool-detail projection facade used by
`agent/tool_details.rs` and keeps these functions stable:

- `tool_input_detail`;
- `tool_output_detail`;
- `tool_input_summary`;
- `truncate_preview` or an equivalent re-export used by `tool_details.rs`.

No caller outside Agent tool-detail internals should learn the new module
layout.

## Internal Module Boundary

Create a focused `agent/tool_details_sanitizer.rs` module for display safety and
preview policy.

It owns:

- preview truncation length and implementation;
- sensitive-key classification;
- scalar summary sanitization;
- command summary sanitization;
- command-array summary normalization;
- shell launcher detection;
- path leaf summaries;
- path-like detection;
- per-field summary classification.

It must not own:

- construction of `ActivityToolInput`;
- construction of `ActivityToolOutput`;
- selection of which raw input/output fields become top-level protocol fields;
- ACP `ToolCall` projection;
- protocol model definitions.

`tool_details_io.rs` owns:

- parsing raw JSON objects;
- selecting top-level input/output fields;
- excluding fields that are already represented at the top level;
- sorting and limiting extra scalar fields;
- returning `ActivityToolInput` and `ActivityToolOutput`.

## Visibility

Keep sanitization functions `pub(super)` only where needed by
`tool_details_io.rs` or `tool_details.rs`. Do not expose sanitizer helpers
crate-wide unless a real caller exists in this slice.

## Behavior Invariants

This slice must preserve:

- sensitive fields redacting to `[redacted]`;
- command token redaction after sensitive flags or keys;
- environment-like `name=value` redaction for sensitive names;
- path-like strings reducing to leaf summaries;
- command-array shell launcher handling for `sh -lc`, `bash -lc`, and `zsh -lc`;
- output preview truncation at the current length;
- empty-string filtering;
- scalar field ordering and maximum count;
- top-level input/output field selection;
- `ActivityToolInput`, `ActivityToolOutput`, and `ActivityToolField` shapes;
- existing `tool_call_preview_does_not_expose_raw_fields_or_full_diff_paths`
  behavior.

## Out Of Scope

- No new redaction rules.
- No changed truncation length.
- No changes to `agent/tool_details.rs` beyond import location if needed.
- No changes to ACP event mapping, normalized Agent events, protocol models,
  Frontend rendering, storage, or App Shells.

## Review Requirements

`$doomsday-review` must check at least:

- no sanitizer helper is exposed more widely than needed;
- protocol shape construction remains in `tool_details_io.rs`;
- redaction/path/command behavior stays covered by existing tests;
- no sensitive raw values become visible in summaries or extra fields;
- the split does not introduce string handling duplication.

## Verification Plan

Run:

- `cargo test -p openaide-runtime tool_call_preview_does_not_expose_raw_fields_or_full_diff_paths -- --nocapture`
- `cargo test -p openaide-runtime`
- `cargo fmt --all --check`
- `npm run check`
- `git diff --check`
