# 301 Project Identity Canonicalization

## Scope

Next A6 slice: make Project identity a single Backend-owned policy instead of
letting snapshots and task creation hash raw workspace-root strings directly.

## Contract

- No new Backend/Frontend protocol methods in this slice.
- App Server owns the mapping from workspace-root path text to:
  - canonical project key;
  - stable `ProjectId`;
  - safe display label;
  - canonical task workspace root.
- Project collection snapshots, task navigation summaries, and
  `TaskCreateParams.projectId` resolution must use the same identity policy.
- Equivalent lexical path spellings such as trailing separators, `.` segments,
  and in-root `..` segments must produce the same Project id and dedupe in
  snapshots.
- The resolver must return the canonical workspace root for new Tasks created
  from a Project id.

## Non-Goals

- No durable Project records yet.
- No workspace picker redesign.
- No filesystem permission probing or symlink resolution.
- No migration of existing Task records.

## Implementation Plan

- Add a focused Project identity helper under the App Server `projects` module.
- Route Project collection, task navigation, and task creation resolution
  through that helper.
- Add regression tests for equivalent workspace-root spellings in snapshots and
  task creation context.

## Implementation Result

- Added `ProjectIdentity` as the single App Server-owned workspace-root identity
  projection.
- Routed Project collection, task navigation, and Project task-context
  resolution through `ProjectIdentity`.
- Canonicalization is lexical only: it normalizes `.`, trailing separators, and
  in-root `..`; it preserves significant whitespace and does not perform
  filesystem probing or symlink resolution.
- Added regression tests for canonical dedupe, canonical task creation context,
  significant whitespace, and absolute parent traversal above root.

## Review Fixes

- Fixed whitespace trimming so distinct valid workspace-root strings do not
  collapse into the same Project identity.
- Fixed absolute `..` handling so parent traversal cannot escape above root and
  equivalent absolute spellings produce the same Project id.

## Verification

- `$doomsday-review` correctness, requirements/tests, and code-quality passes;
  material findings fixed.
- `cargo fmt --all --check`
- `cargo test -p openaide-runtime project --lib`
- `cargo test -p openaide-runtime`
- `jq empty .workflow/finish-refactor-plan/state.json`
- `git diff --check`
- production Rust source-size scan for files over 300 lines

## Next

Select and grill the next A6 slice after Project identity canonicalization.
