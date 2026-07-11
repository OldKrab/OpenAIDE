# Next Slice Selection: Tool Details Sanitization Split

## Decision

Select the Tool Details sanitization split as the next Backend refactor slice.

## Why This Slice

`agent/tool_details_io.rs` currently combines two responsibilities:

- projecting raw ACP tool input/output JSON into protocol-safe
  `ActivityToolInput` and `ActivityToolOutput` records;
- applying display safety policy for sensitive keys, command previews, path
  leaf summaries, scalar sanitization, and preview truncation.

The second responsibility is product safety and UX policy, not raw I/O
projection. Splitting it gives future tool-detail and diagnostics work a clear
place to review redaction and display rules without touching projection shape.

## Proposed Boundary

Keep `agent/tool_details_io.rs` as the raw tool-detail projection facade for:

- `tool_input_detail`;
- `tool_output_detail`;
- `tool_input_summary`;
- the stable helper currently used by `agent/tool_details.rs` for content
  preview truncation.

Move redaction and display sanitization policy into a focused child/helper
module, including:

- sensitive-key classification;
- command summary sanitization;
- scalar summary sanitization;
- path leaf summaries;
- path-like detection;
- preview truncation.

## Out Of Scope

- No behavior changes to redaction, command summary, path summary, field
  ordering, truncation length, or `ActivityTool*` shapes.
- No changes to ACP event projection or normalized Agent events.
- No protocol, storage, Frontend, or App Shell changes.
- No broad `tool_details.rs` rewrite.

## Next Step

Grill and record the accepted API contract for this slice before implementation.
