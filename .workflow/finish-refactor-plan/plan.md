# Finish OpenAIDE Refactor Plan

## Goal

Finish the accepted OpenAIDE refactor plan one slice at a time, using explicit planning,
repeated review, implementation, integration, and verification so the resulting code is
production-quality, well-tested, encapsulated, loosely coupled, and aligned with the
Backend/Frontend architecture.

## Success Criteria

- Each implementation slice starts from a recorded module/API plan or updates
  `docs/refactor-plan.md` / ADRs before code changes.
- Each slice has clear module ownership, narrow interfaces, and no product decisions in
  the wrong layer.
- App Server, App Server Protocol, Frontend, App Shell, storage, transport, Agent
  runtime, and shell capability concerns stay separated.
- Frontend remains responsive by design: slow Backend or Agent work is represented as
  renderable pending/preparing/progress/error state.
- New code has focused tests at the boundary where behavior can regress.
- Root validation passes before a slice is considered done.
- Review stops only after repeated review finds no material correctness,
  requirements, encapsulation, or test gaps for the current slice.

## Current Context

- Branch: `refactor/app-architecture-plan`.
- The accepted A0-A9 architecture backlog is implemented and committed.
- Current mode is completion audit, not another open-ended packet loop.
- Root context sources are `CONTEXT.md`, `PRODUCT.md`, `DESIGN.md`,
  `docs/refactor-plan.md`, ADRs under `docs/adr/`, and relevant implementation files.

## Constraints

- Do not preserve legacy code for compatibility when a replacement boundary is accepted.
- Do not expose implementation provenance, private domains, temporary hosts, local paths,
  or conversation-specific setup in source, docs, metadata, UI, comments, or commits.
- Keep hand-written production source files at or below 400 logical lines; split before
  300 when growth is obvious. Tests and generated files are exempt.
- Rust test bodies should live in separate test files where practical.
- Use Rust protocol types as App Server Protocol source of truth and regenerate checked-in
  TypeScript bindings after protocol changes.
- Use subagents for bounded review or implementation packets when useful and available.
- Ask for approval before destructive repository operations, force pushes, broad deletes,
  mass renames, external publication, or secret/credential access.

## Risks

- Mixing legacy shell/webview contracts with the new App Server Protocol.
- Letting `protocol_edge` or Frontend handlers accumulate product workflow decisions.
- Overfitting to VS Code while Web/Desktop shells remain planned.
- Emitting UI-visible state before durable Backend acceptance.
- Creating broad modules with weak encapsulation because the refactor is large.
- Adding tests that prove helpers but not the product boundary behavior.

## Approval Required

Approval is required before:

- deleting large legacy subsystems,
- broad codemods or mass renames,
- irreversible Git operations,
- starting many long-running subagents,
- touching secrets or credentials,
- publishing, deploying, or changing external systems.

Normal local edits, local tests, generated protocol bindings, and workflow artifact updates
do not require extra approval.

## Work Packets

Historical packets through `P231` used separate entries for selection, contract,
implementation, review, verification, and next-slice selection. That made the packet
counter look like product progress and wasted context.

After `P231`, use one packet per implementation slice. Each slice result file contains:

- `contract`: ownership, public API, dependencies, forbidden behavior, failure modes, and
  focused tests.
- `implementation`: files changed and boundary decisions.
- `review`: local self-review plus `$doomsday-review` findings and fixes.
- `verification`: commands run, logs saved, failures, and skipped checks.
- `commit`: final commit hash and scope.
- `next`: next bounded slice candidate.

Do not create separate workflow packets or result files for normal per-slice phases.
Create separate planning artifacts only when the API/product boundary changes materially
or a slice is too large to implement safely.

## Completion Mode

The A0-A9 architecture backlog is complete. Do not continue selecting packets just
because historical notes contain "next" wording. Finish with one bounded completion
audit:

- verify the workflow state marks A0-A9 completed;
- audit current production code for active legacy product bridge fallbacks, source-size
  violations, and obvious `TODO` / `unimplemented` placeholders;
- run root verification (`npm run check` and `npm test`) or cite the latest green run
  when the final change is docs-only;
- record the result in `results/428-completion-audit.md`;
- commit the workflow/doc update;
- mark the active goal complete if no concrete architecture-plan gap remains.

If this audit finds a real gap, create exactly one concrete follow-up packet with the
smallest implementation scope needed to close it. Do not restart broad review loops.

## Integration Policy

- Keep each slice small enough to explain in one review.
- Prefer deep modules with narrow, typed APIs over broad service bags.
- Put product invariants in core/product workflow modules, not edge handlers.
- Keep shell-specific bridge contracts out of `packages/app-server-client`; that package
  remains App Server Protocol bindings and thin helpers.
- Accept subagent results only after checking exact files and lines locally.
- If review packets disagree, inspect the authoritative source and record the decision in
  the result note.

## Review Stop Rule

For each slice, review stops only when all are true:

- At least one local self-review pass has checked architecture boundaries, state machines,
  error cases, and tests.
- At least one independent review pass has run. Use subagents when the host supports them;
  otherwise simulate separate packet passes in `results/`.
- All High and Medium findings are fixed or explicitly rejected with proof.
- Any remaining Low findings are documented as acceptable residual risk or fixed.
- `npm run check`, `npm test`, and slice-specific build/test commands pass, unless a
  skipped command is explicitly justified in the final report.

## Review Loop Controller

Reviews are bounded. Do not review endlessly.

For each implementation slice:

1. Run review round 1 after implementation:
   - one local self-review,
   - one independent correctness pass,
   - one independent requirements/tests pass,
   - one independent code-quality pass.
2. Fix all accepted High and Medium findings.
3. Run review round 2 against only the fixes, the original accepted findings, and the
   original high-risk areas.
4. Stop after round 2 if there are no new High or Medium findings.
5. Run round 3 only if round 2 finds a new High/Medium issue caused by the fixes, or if
   two review passes materially disagree about the same boundary.
6. Stop after round 3 unless a Critical data-loss, privacy, security, build-breaking, or
   architecture-breaking issue remains.
7. If a Critical issue remains after round 3, mark the slice blocked, record the blocker,
   and stop implementation until the blocker is replanned.

Low findings cannot keep the loop open by themselves. Fix them when cheap; otherwise
record them as residual risk in the packet result.

Review rounds must get narrower over time. Round 1 may inspect the whole slice. Round 2
reviews accepted fixes plus original risk areas. Round 3 reviews only the remaining
blocker or disagreement.

For low-risk mechanical splits, the independent review pass may be one targeted
`$doomsday-review` pass over the slice instead of three separate review packets. Use the
three-area split only for behavior changes, protocol changes, storage/lifecycle changes,
security/privacy boundaries, or when the first review finds material issues.

## Verification

Default verification for every code slice:

- `cargo fmt --all`
- `npm run protocol:generate` after protocol type changes
- `npm run protocol:check`
- `npm run check`
- `npm test`
- package or app build affected by the slice, commonly `npm run build:frontend`

Additional verification:

- Browser/UI smoke tests for shared Frontend or shell behavior.
- Concurrency/state-machine tests for lifecycle, storage, request routing, and Agent work.
- Regeneration drift checks for generated bindings.
- Source-size scan for production files near 300/400 lines.

Keep command output capped. Prefer redirecting long logs to `/tmp` and reporting pass/fail
plus the failure tail. Run targeted checks during implementation; run the full default
verification once before commit and again only if accepted fixes touch shared risky code.

## Reusable Artifacts

- Keep this workflow under `.workflow/finish-refactor-plan/`.
- Add one slice result under `.workflow/finish-refactor-plan/results/`.
- If this loop works well, distill it into `.workflow/recipes/refactor-slice-loop.md`.
