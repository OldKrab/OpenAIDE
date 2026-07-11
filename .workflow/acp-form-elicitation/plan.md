# ACP form elicitation implementation

Goal: implement generic unstable ACP form elicitation end to end, using the approved inline Question design.

Success criteria:

- Advertise ACP `elicitation.form` and handle the pinned SDK's typed unstable request/response shapes.
- Route session-scoped form requests to the bound Task, including outside an active turn; cancel non-session scope and reject unsupported modes or invalid schemas.
- Normalize and budget the full documented restricted schema without exposing raw ACP payloads to Frontend.
- Reuse Task-scoped interactive request semantics: Waiting status, multiple independent requests, first valid response wins, reconnect/resync, cancellation, and durable history.
- Render the approved unified rounded Question form, with no field numbers and quiet dividers between fields, plus validation, responding, resolved preview, and closed/error states.
- Generate protocol bindings and pass focused Rust/TypeScript tests, protocol checks, source-size checks, target redeploy, and desktop/mobile browser verification.

Constraints:

- Preserve all existing user changes in the dirty worktree.
- Keep permission and elicitation domain models distinct.
- Keep App Server authoritative for schema and response validation.
- Do not depend on the cloned codex-acp repository.
- Do not mutate or restart the driver instance.

Risks:

- ACP elicitation is unstable and the pinned SDK may differ from live documentation.
- Existing permission waiting holds an emission lock; elicitation must not serialize concurrent pending requests.
- Protocol, persistence, and frontend projection changes cross several typed seams.
- Submitted values are durable Task history by explicit product decision, but must remain redacted from diagnostics.

Verification:

1. Focused schema/response validator tests.
2. App Server protocol, runtime, persistence/reload, cancellation, concurrency, and stale-response integration tests.
3. Frontend mapping, reducer, rendering, validation, and response tests.
4. `npm run protocol:generate`, `npm run protocol:check`, `npm run check:source-size`, relevant Rust/TypeScript suites, then broader checks.
5. `npm run web:target:restart` and Playwright desktop/mobile QA.
