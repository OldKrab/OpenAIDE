# AGENTS.md

This file is the short operating guide for agents working in this repo. Keep detailed product and architecture policy in docs, not here.

## Useful files

- Product language: `CONTEXT.md`.
- Product purpose and UX principles: `PRODUCT.md`.
- Visual system: `DESIGN.md`.
- Other ADRs under `docs/adr/` when touching their area.

## Working Rules

- Document code. Leave comments on improtant funcitons, classes, not obvious code pieces.
- Use logging in code. It is essential for future debugging.
- Before committing, inspect the complete staged diff for secrets, credentials, personal domains, email addresses, usernames, home-directory paths, machine-specific configuration, and other sensitive or personal data. Keep local machine configuration in ignored files. Report any findings and unresolved failing checks before committing.
- Keep Backend and Frontend concerns separate. App Server owns product state and workflow decisions; Frontend owns rendering and ephemeral presentation state.
- When Rust App Server Protocol change, regenerate TypeScript bindings with `npm run protocol:generate` and verify with `npm run protocol:check`.
- For non-trivial architecture or API design, discuss the approach first and state the next planned step.
- Treat simplicity as a primary constraint for every code and design change, including features, fixes, and refactors. Do not preserve existing complexity merely because it exists. Prefer one owner, one state representation, one ordering mechanism, one validation pass, and visible failure with explicit recovery. If implementation would expand an agreed design, stop and discuss it first.
- During the current architecture replacement, provide no compatibility with superseded OpenAIDE implementation details, internal or App Server interfaces, protocol shapes, or persisted development-data formats. Remove old paths completely; do not add adapters, migrations, dual reads/writes, or fallback deserialization for them.


## Bugs And TDD

- When the user reports a bug, use TDD: reproduce or add a failing regression test first, then fix, then rerun the test.
- Regression tests should catch the bug at the closest real user, protocol, or storage boundary. Do not duplicate existing coverage.
- Do not accept mocks that hide protocol semantics. If ACP sends chunks, updates, or replayed history, tests must model chunks, updates, and replayed history.

## Prototyping

- Follow `docs/prototyping.md` for disposable UI and logic prototypes.

## Source Size And Tests

- Hand-written production source files must stay at or below 800 logical lines. Start splitting before 600 lines when a file is still growing.
- Test files, generated files, lockfiles, snapshots, fixtures, vendored files, and machine-generated bindings are exempt from the production source size limit.
- Do not add new production logic to a file that already exceeds the limit; first extract cohesive modules until the production file is back under the limit.
- Rust test bodies must live in separate test files, not inline `#[cfg(test)] mod tests` blocks in production modules. Prefer crate-level integration tests under each crate's `tests/` directory; when private access is required, put only a tiny `#[cfg(test)] mod tests;` declaration in the production file and put the test body in a sibling test file.
- Name a private Rust unit-test file `<module>_tests.rs` and load it with `#[cfg(test)] #[path = "<module>_tests.rs"] mod tests;`. Use a test directory only when it contains multiple behavior-focused files; do not create a directory solely for one `tests.rs`.
- Shared Rust integration-test helpers belong under a subdirectory module such as `tests/common/mod.rs`, not directly in `tests/common.rs`, so Cargo does not treat helpers as a standalone integration test crate.

## Verification

Run the narrowest relevant checks first, then broaden when contracts or shared behavior change.
- Rust format: `cargo fmt --all --check`
- Rust tests: `cargo test -p openaide-app-server`
- Rust lint: `cargo clippy -p openaide-app-server --all-targets -- -D warnings`
- TypeScript/build checks: `npm run check`
- Full build: `npm run build`
- Workspace tests: `npm run test --workspaces --if-present`
