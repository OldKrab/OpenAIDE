# Whole project audit and repair

## Goal

Audit the complete OpenAIDE repository at baseline commit `9b318c8`, prove and repair high-confidence correctness defects, simplify architecture where the existing seams are weak, and bring the running Target UI to production-quality desktop and narrow-width behavior.

## Success Criteria

- Every hand-written source area is assigned to an audit packet and represented in the coverage inventory.
- Baseline automated checks are recorded; each accepted bug fix begins with a failing behavioral regression test where a viable seam exists.
- Architecture findings are tied to concrete ownership, interface, coupling, or maintainability evidence and repaired when the benefit is material.
- The Target web app is exercised in a real browser at desktop and constrained widths, including loading, task navigation, chat, composer, settings, permissions, errors, and overflow-sensitive states available in the local data.
- Protocol changes regenerate and validate TypeScript bindings.
- Narrow checks, repository checks, builds, Rust formatting/lints/tests, and browser smoke checks pass, or remaining failures are documented with proof.
- The Driver instance on port 5474 is never mutated.

## Current Context

- Repository: the current repository root
- Branch: `refactor/app-architecture-plan`
- Preserved baseline: `9b318c8` (`checkpoint: preserve current worktree`)
- Scope: 663 tracked/non-ignored files before generated/runtime directories.
- Product constraints come from `CONTEXT.md`, `PRODUCT.md`, `DESIGN.md`, ADR 0022, and `docs/refactor-plan.md`.

## Constraints

- Preserve App Server ownership of product state and the typed App Server Protocol seam.
- Keep hand-written production files at or below 400 logical lines.
- Use target-only self-development commands; never restart or rebuild the Driver.
- Do not claim that all possible bugs are discoverable. Report coverage and residual risk honestly.
- Do not revert the user's baseline work. Concurrent packets must use disjoint ownership.

## Risks

- Lifecycle, persistence, cancellation, recovery, permissions, and streamed ACP updates can create data-loss or stale-state bugs.
- Frontend derived state and asynchronous callbacks can diverge from authoritative App Server snapshots.
- Broad rewrites can hide regressions unless done as vertical test-first slices.
- Local target configuration currently contains machine-specific values and must not leak into product-facing surfaces.

## Approval Required

The user explicitly authorized breaking changes and broad rewrites. Destructive Git operations, force pushes, Driver mutation, credential access, and external publication remain out of scope and would require separate approval.

## Work Packets

1. Repository-wide correctness audit, with explicit coverage across Rust backend/protocol, frontend/client contracts, VS Code host, web host, deploy, and scripts.
2. Repository-wide requirements and behavioral-test audit against product, design, ADR, protocol, lifecycle, persistence, accessibility, and recovery invariants.
3. Repository-wide architecture and code-quality audit, including ownership, module depth, type seams, file size, duplication, atomicity, and orchestration complexity.
4. Target browser UX, visual design, responsive behavior, accessibility, and interaction audit.
5. Integration: verify proofs, select fixes, execute vertical TDD slices, regenerate contracts, and run broad verification.

## Integration Policy

Accept only findings with a reproducible failure, a violated documented invariant, or exact structural proof. Merge duplicate root causes. Prefer fixes that deepen the owning module and delete scattered policy. Reject speculative rewrites that add interface surface without verified leverage.

## Verification

Run the narrowest test for each slice, then `npm run protocol:check` where applicable, workspace checks/tests/build, `cargo fmt --all --check`, App Server tests, App Server clippy, Target redeploy, and Playwright desktop/narrow smoke checks.

## Reusable Artifacts

Keep packet coverage and final evidence in this workflow directory. Add a reusable recipe only if the audit exposes a repeatable gap not already encoded in repository rules.
