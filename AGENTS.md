# AGENTS.md

This file is the short operating guide for agents working in this repo. Keep detailed product and architecture policy in docs, not here.

## Read First

- Product language: `CONTEXT.md`.
- Product purpose and UX principles: `PRODUCT.md`.
- Visual system: `DESIGN.md`.
- Backend/Frontend architecture and attachment rules: `docs/adr/0022-backend-frontend-app-shell-architecture.md`.
- Larger refactor sequencing: `docs/refactor-plan.md`.
- Other ADRs under `docs/adr/` when touching their area.

## Working Rules

- Build production-quality code with clear names, modular structure, no duplication, and tests at the user-visible or protocol boundary.
- Push changes through a feature branch and a pull request. Never push directly to `main`; before pushing, run `npm run ci` and report any failures instead of bypassing the gate.
- Before committing, inspect the complete staged diff for secrets, credentials, personal domains, email addresses, usernames, home-directory paths, machine-specific configuration, and other sensitive or personal data. Keep local machine configuration in ignored files. Report any findings and unresolved failing checks before committing.
- Keep Backend and Frontend concerns separate. App Server owns product state and workflow decisions; Frontend owns rendering and ephemeral presentation state.
- Keep App Server, App Server Protocol, App Shells, storage, transport, Agent runtime, and shared Frontend surfaces in their proper modules.
- Use the typed App Server Protocol seam. Do not add untyped method strings, `unknown` protocol payload plumbing, or shell-specific product protocols.
- When Rust App Server Protocol types, method maps, event payloads, or envelope shapes change, regenerate TypeScript bindings with `npm run protocol:generate` and verify with `npm run protocol:check`.
- If App Server or Web App bootstrap code changes while a Web App dev instance is running, redeploy with `npm run web:redeploy` before browser verification.
- Do not expose implementation provenance in project-facing docs, UI, comments, package metadata, or commit messages.
- Do not hard-code local development URLs, machine-specific paths, private domains, usernames, temporary preview hosts, or conversation-specific setup details in source, tests, docs, UI text, comments, package metadata, or commit messages.
- Avoid catch-all modules. Document invariants where they matter: protocol fields, storage atomicity/cursors, task lifecycle, stale response guards, and agent boundaries.

## Planning

- For non-trivial architecture or API design, discuss the approach first and state the next planned step.
- A clear imperative implementation request is approval to proceed unless it is destructive, safety-sensitive, conflicts with rules, or has unclear scope.
- For hard refactor planning, use `docs/refactor-plan.md` as the living top-level plan.

## Bugs And TDD

- When the user reports a bug, use TDD: reproduce or add a failing regression test first, then fix, then rerun the test.
- Fixes should be clean and thought through; avoid broad rewrites unless the bug proves the module boundary is wrong.
- Regression tests should catch the bug at the closest real user, protocol, or storage boundary. Do not duplicate existing coverage.

## UI Work

- Use the `impeccable` skill before significant frontend changes and again for audit/polish.
- Match `DESIGN.md` and `.impeccable/design.json`; do not duplicate design rules here.
- Verify desktop and narrow/mobile widths when changing layout.
- For UI QA, use Playwright screenshots and inspect them; look for UX polish issues, flicker, overflow, and confusing states, not just functional bugs.

## Prototyping

- Follow `docs/prototyping.md` for disposable UI and logic prototypes.
- Keep UI prototype implementations under the ignored `packages/frontend/prototypes/` directory. Never force-add or commit them.
- Reuse production components through the prototype harness and use Target hot reload for review; do not add temporary prototype routes to the primary application.

## Attachment Policy

- Do not keep detailed attachment rules here. Follow `docs/adr/0022-backend-frontend-app-shell-architecture.md`.
- Current v1 direction: Web App supports App Server-backed file browsing for local file references and explicit browser upload/paste for uploaded files such as images. App Server-owned handles, validation state, safe labels, and send-time checks are authoritative.

## Source Size And Tests

- Hand-written production source files must stay at or below 800 logical lines. Start splitting before 600 lines when a file is still growing.
- Test files, generated files, lockfiles, snapshots, fixtures, vendored files, and machine-generated bindings are exempt from the production source size limit.
- Do not add new production logic to a file that already exceeds the limit; first extract cohesive modules until the production file is back under the limit.
- Rust test bodies must live in separate test files, not inline `#[cfg(test)] mod tests` blocks in production modules. Prefer crate-level integration tests under each crate's `tests/` directory; when private access is required, put only a tiny `#[cfg(test)] mod tests;` declaration in the production file and put the test body in a sibling test file.
- Name a private Rust unit-test file `<module>_tests.rs` and load it with `#[cfg(test)] #[path = "<module>_tests.rs"] mod tests;`. Use a test directory only when it contains multiple behavior-focused files; do not create a directory solely for one `tests.rs`.
- Shared Rust integration-test helpers belong under a subdirectory module such as `tests/common/mod.rs`, not directly in `tests/common.rs`, so Cargo does not treat helpers as a standalone integration test crate.

## Verification

Run the narrowest relevant checks first, then broaden when contracts or shared behavior change.

- App Server/protocol work needs integration tests for the real external shape, including streamed chunks, partial updates, stale responses, permission branches, failed operations, retries, cleanup paths, and persisted state reloads.
- Do not accept mocks that hide protocol semantics. If ACP sends chunks, updates, or replayed history, tests must model chunks, updates, and replayed history.
- Rust format: `cargo fmt --all --check`
- Rust tests: `cargo test -p openaide-app-server`
- Rust lint: `cargo clippy -p openaide-app-server --all-targets -- -D warnings`
- TypeScript/build checks: `npm run check`
- Full build: `npm run build`
- Workspace tests: `npm run test --workspaces --if-present`
