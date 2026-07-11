# Protocol TypeScript Generator Split

## Contract

Split focused generator helpers out of
`openaide-rs/app-server-protocol/src/typescript.rs` while preserving
`typescript::bindings()` as the stable public API and preserving generated
TypeScript output.

Ownership:

- `typescript.rs`: generator facade, TS config, header, output order, and public
  `bindings()` entry point.
- `typescript/method_constants.rs`: emitted method constant declarations sourced
  from `crate::methods`.
- `typescript/declarations.rs`: emitted `ts_rs` declarations sourced from Rust
  protocol records.
- `typescript/method_maps.rs`: emitted typed method union, params/result maps,
  typed request/response aliases, and convenience response aliases.

Do not change protocol records, method names, generated TypeScript contents,
public Rust module names, generated binding file paths, or App Server/client
runtime behavior in this slice.

Focused tests:

- Existing `typescript::tests::generated_bindings_include_protocol_method_maps`
  remains the behavior suite for generated binding structure.
- `npm run protocol:generate` and `npm run protocol:check` cover generated
  binding drift.

## Implementation

Implemented the split by keeping `bindings()` as the facade and moving constants,
type declarations, and method-map output into private modules under
`src/typescript/`.

Production source sizes after split:

- `typescript.rs`: 21 lines.
- `typescript/declarations.rs`: 169 lines.
- `typescript/method_constants.rs`: 51 lines.
- `typescript/method_maps.rs`: 53 lines.

## Review

`$doomsday-review`:

- Correctness/spec/tests: no findings.
- Code quality: local pass found no findings.

Note: the review called out only that the new split files were untracked before
commit staging.

## Verification

Focused checks already run:

- `cargo fmt --all --check`: pass.
- `cargo check -p openaide-app-server-protocol`: pass.
- `cargo test -p openaide-app-server-protocol typescript::tests -- --nocapture`: pass.
- `npm run protocol:generate`: pass.
- `npm run protocol:check`: pass.

Final checks:

- `npm run check`: pass.
- `npm test`: pass.
- `git diff --check`: pass.
- `jq empty .workflow/finish-refactor-plan/state.json`: pass.
- Changed production source-size scan: largest split file is
  `typescript/declarations.rs` at 166 lines.

## Commit

This commit: `refactor: split protocol typescript generator`.

## Next

After this slice is committed, select the next compact refactor slice from the
current plan and architecture/file-size pressure.
