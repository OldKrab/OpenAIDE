# Smooth Agent Text Streaming

Goal: Deliver durable ACP text chunks to Chat immediately and present them as stable, smooth Markdown without falsifying protocol order.

Success criteria:

- A frontend subscriber observes Agent text before `session/prompt` completes.
- App Server emits committed append/chunk/finalization events instead of rebuilding full Task snapshots for every chunk.
- Local HTTP provides push delivery; polling remains recovery only.
- Live text uses bounded, word-aware presentation with stable Markdown, ordered activity barriers, reduced-motion behavior, and no replay animation.
- Chat offers Jump to latest whenever the viewport is away from the bottom.
- Cancellation, permission, reconnect, resync, hidden-tab, concurrent-message, and completion paths cannot strand or reorder text.
- Focused tests, workspace checks, desktop/narrow browser QA, and target redeploy pass.

Constraints:

- Preserve unrelated dirty work, including `packages/frontend/prototypes/`.
- Do not mutate or restart the Driver instance.
- Persist exact source text; presentation state is Frontend-only and disposable.
- Keep production source files within repository size limits.

Work packets:

1. Push transport: failing public connection test, then immediate event delivery with recovery polling.
2. Committed chat deltas: failing protocol boundary test, then append/chunk/finalization publication.
3. Live presentation: failing pure/component tests, then Markdown-aware reveal, barriers, copy, and scroll behavior.
4. Integration: protocol generation, end-to-end fixture, browser QA, target restart, broad verification.

Integration policy: packet owners edit disjoint modules, never revert concurrent changes, and report conflicts before crossing ownership.

Verification: narrow RED/GREEN tests per slice, Rust tests and lint, `npm run check`, workspace tests/build, Playwright wide/narrow/reduced-motion checks, then target restart.
