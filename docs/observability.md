# Observability

OpenAIDE diagnostics exist to explain lifecycle decisions and failures without recording the user's work. Production logs are structured metadata, not transcripts.

## What to log

Log at ownership seams where a future investigator otherwise could not reconstruct what happened:

- process, connection, Native Session, prompt, Task, subscription, and request lifecycle transitions;
- external I/O attempts and their outcomes;
- persisted state transitions, recovery, retry, fallback, and reconciliation decisions;
- rejected, ignored, stale, orphaned, or unsupported inputs;
- latency or queue pressure only as aggregate durations, counts, and thresholds.

Pure computation, ordinary rendering, successful getters, and high-frequency chunks stay quiet. A successful hot path should have one lifecycle completion event, not one event per internal step.

## Event shape

Use stable `snake_case` event names describing facts, such as `acp_session_update_received` or `task_recovery_failed`. Prefer paired lifecycle names when both ends matter: `_started` / `_completed`, `_received` / `_committed`.

Fields should be typed metadata:

- stable correlation identifiers such as Task, turn, Native Session, request, connection, tool-call, or client IDs;
- method, state, outcome, reason code, capability, count, duration, and byte size;
- safe enumerated error class or failure stage.

The layer that owns the decision owns the event. Adjacent layers should not duplicate the same success fact.

## Sensitive data

Default logs must not contain prompts, user or agent message text, command text, terminal output, file contents, filesystem paths, environment values, credentials, tokens, secrets, email addresses, or arbitrary error messages.

Sensitive field names are fully redacted. Do not rely on pattern replacement inside free-form values. When an error matters, log a stable `error_kind`, `stage`, or protocol code and keep the original error in the user-facing or typed error path.

ACP traces are an explicitly enabled diagnostic artifact and may contain protocol payloads. They must be routed to the owning Native Session or Task, clearly marked sensitive, excluded from ordinary logs, and handled by support export redaction.

## Support Export

The VS Code Support Export is the user-shareable diagnostic boundary. It writes
one ZIP with the allowlisted runtime snapshot, minimal platform/version facts,
and the newest complete records from the last 24 hours of Extension and App
Server logs, capped at 2 MB per source. Missing or malformed sources are noted
without preventing a partial export.

Export processing applies a second strict allowlist instead of copying local
logs directly. Custom Agent identifiers are replaced with export-local tokens;
arbitrary fields and error text are discarded; known failures may receive a
controlled product-authored summary. Prompts, Chat, file contents and paths,
terminal output, environment variables, secrets, raw protocol payloads, and ACP
traces are never included. The command saves locally and only opens a GitHub
issue after explicit user action; it does not upload diagnostics itself.

## Levels

- `info`: meaningful lifecycle or ownership transition.
- `warn`: recoverable failure, retry, stale input, fallback, unexpected but contained state, or dropped observer.
- `error`: requested work failed, durable state may be inconsistent, or recovery could not complete.

Do not use severity to compensate for missing correlation or vague event names.

## Delivery invariants

Recognized protocol and lifecycle events must never disappear silently. Each event is processed, rejected with a reason, retried, or recorded as an orphan for recovery. Diagnostic instrumentation must distinguish received, routed, projected, and committed stages when those stages have independent failure modes.

## Audit baseline

The July 2026 repository audit covered the App Server, ACP integration, VS Code Extension, Web App Shell, shared Frontend, and App Server client transport.

- The App Server owns durable Task, Native Session, request, storage, recovery, and ACP projection events. Its logger redacts sensitive fields centrally and reports logger-open/write failure through a metadata-only stderr fallback.
- Raw ACP `session/update` traces are routed by ACP session ID to the owning Task trace. Default logs record tool-call status transitions, while repeated partial chunks remain quiet.
- The VS Code Extension records process, handoff, RPC, workspace-sync, Webview action, and Webview telemetry boundaries. Its logger normalizes events and applies central field redaction.
- The Web App Shell records process handoff and heartbeat transitions. The browser shell preserves allowlisted Webview telemetry instead of silently dropping it.
- The shared Frontend reports App Server initialization, event-stream loss, state reset, subscription retry, and recovery. It sends error classes, never browser-specific error text.
- The App Server client remains a transport library: it signals disconnects and state resets to its owner instead of creating a competing log sink. Lifecycle logs belong to the Frontend or App Shell that can add surface and Task correlation.

Reducers, mapping functions, render-only components, successful getters, expected best-effort cleanup, observer removal, and per-chunk message flow are intentionally quiet. Add instrumentation at their owning caller only when a failure changes product state or recovery behavior.
