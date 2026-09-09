# OpenAIDE Agent Guide

## Orient

Read the narrow source of truth before changing its area:

- Product terms: `CONTEXT.md`.
- Product intent and UX principles: `PRODUCT.md`.
- Visual work: `DESIGN.md`.
- Task lifecycle, Chat, task navigation, or replica behavior: `docs/task-chat-flow.md` and the relevant linked ADRs. This is an accepted specification; discuss and obtain explicit user agreement before changing its behavior or requirements.
- Frontend work: `packages/frontend/AGENTS.md`.
- Disposable prototypes: `docs/prototyping.md`.
- A decision governed by an ADR: the relevant file in `docs/adr/`.

## Shape

- Establish the owning layer and contract before implementation. App Server owns durable product state, workflow decisions, persistence, and ordering; Frontend renders authoritative state and owns only ephemeral presentation.
- Keep the design tight: one owner, state representation, ordering mechanism, and validation pass. Prefer an explicit failure and recovery path to hidden coordination or retries.
- Discuss the approach before a non-trivial architecture or API change. Stop for agreement when implementation would expand an accepted design.
- Keep user-facing product behavior shared across app shells; put shell chrome, routing, and capabilities behind narrow composition points.
- When changing the Rust App Server Protocol, regenerate and check the TypeScript bindings with the repository scripts.

## Write

- Leave concise comments on public and non-obvious code: explain ownership, invariants, lifecycle boundaries, and tradeoffs. Follow the Observe rules for runtime diagnostics.
- Keep hand-written production files below 800 logical lines; extract a cohesive module before extending a file that exceeds it.

## Observe

- Treat lifecycle and boundary logging as part of implementation. For every asynchronous operation crossing the Web Shell, Frontend, App Server, transport, persistence, or Agent boundary, emit structured start and terminal events with a stable operation name, safe correlation identifiers, outcome, duration, retry or attempt count, and classified error.
- Record state transitions and waits that can explain user-visible latency. Keep healthy polling loops quiet; log the poll wake, timeout, retry, terminal failure, and meaningful batch instead of every iteration.
- Keep diagnostics metadata-only and production-safe: redact prompts, content, secrets, tokens, credentials, environment values, paths, URLs, and free-form error messages. Make loggers injectable where tests need to assert success, failure, and retry paths.
- When diagnosing a delay, follow one correlation identifier across every layer and verify the first and last event for each lifecycle stage before proposing a cause.

## Prove

Tests and verification: read [the testing guide](docs/testing.md) when writing tests, diagnosing test failures, or validating changes.

## Hand off

- Before a commit, inspect the complete staged diff for credentials, personal or machine-specific data, and local paths. Keep local configuration ignored.
- Report unresolved checks and any security-sensitive findings with the change.
