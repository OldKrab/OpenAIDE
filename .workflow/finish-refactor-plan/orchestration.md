# Orchestration: Finish OpenAIDE Refactor Plan

## Execution Rules

1. Start every turn by checking the newest user request, `git status --short --branch`,
   and the active workflow state.
2. Read `CONTEXT.md`, `PRODUCT.md`, `DESIGN.md`, `docs/refactor-plan.md`, relevant ADRs,
   and touched implementation files before planning a slice.
3. For each slice, use one workflow packet and one result file. The packet contains these
   phases internally instead of creating separate packets for selection, contract,
   implementation, review, verification, and next-slice bookkeeping:
   - Select the next bounded slice from `docs/refactor-plan.md`.
   - Record the slice contract: ownership, public API, forbidden dependencies, failure
     behavior, and focused tests.
   - Review the contract locally; use subagents only when the boundary is non-obvious or
     high risk.
   - Implement only that slice.
   - Run a local self-review, then `$doomsday-review` for the implementation.
   - Fix material findings and repeat review only within the bounded review controller in
     `plan.md`.
   - Verify, commit, and record the next planned slice candidate.
4. Keep plans concrete. If a plan names a module, it must say what that module owns,
   what it must not own, and how callers interact with it.
5. Do not ask the user implementation-detail questions unless the answer changes a
   product/API boundary and cannot be discovered from existing context.
6. Every progress update must state what is happening now and what comes next.
7. Packet IDs are historical workflow identifiers, not a count of product steps. After
   `P231`, do not create multiple packet IDs for one slice.

## Branching Rules

- If the worktree has uncommitted changes unrelated to the current slice, do not edit those
  files until they are classified. If they are review fixes from the current slice, finish
  and commit them first.
- If a review finds High or Medium correctness, data safety, privacy, architecture, or test
  gaps, fix them before adding new scope.
- If review reaches the configured round limit, stop reviewing unless a Critical issue
  remains. Record residual Low findings instead of reopening the loop.
- If implementation pressure starts creating catch-all modules, stop and split ownership
  before continuing.
- If a slice needs a broad delete or mass rename, stop for explicit approval.
- If root verification fails from a known flaky test, rerun once serially. If it fails again,
  diagnose or record it as a blocking failure.

## Slice Packet Template

Use one packet per implementation slice after `P231`.

### PNNN-<slice-name>

Objective: Select, contract, implement, review, verify, commit, and hand off one bounded
refactor slice.

Ownership: only files named by the slice contract.

Do:
- Record the slice contract before source edits.
- Keep edits scoped and cohesive.
- Add or update focused tests for behavior and boundaries.
- Regenerate protocol bindings if Rust protocol types change.
- Run `$doomsday-review` after implementation.
- Fix accepted High/Medium findings and repeat review only within the bounded controller.
- Run required checks with capped output.
- Commit only the slice.
- Record the next slice candidate in the result file.

Do not:
- Preserve legacy paths for compatibility after a replacement boundary is accepted.
- Add shell-specific product behavior to shared Frontend or protocol packages.
- Create separate packet IDs for contract, implementation, review, verification, or
  next-slice selection.
- Keep reviewing Low-only findings forever.

Expected output: one committed slice and one result file with `contract`,
`implementation`, `review`, `verification`, `commit`, and `next`.

## Completion Audit

A slice is complete only if:

- docs or ADRs reflect accepted API decisions,
- code follows those decisions,
- review findings are fixed or explicitly rejected,
- verification commands pass,
- work is committed,
- workflow state and result notes are updated.
