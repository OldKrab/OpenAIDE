# P02-api-grill-next-slice

## Objective

Define the next module/API slice enough to implement safely.

## Context

The next slice should come from `docs/refactor-plan.md` module grill queue and current
implementation state. Prefer the smallest slice that unlocks the next meaningful Backend
or Frontend boundary.

## Files / Sources

- `docs/refactor-plan.md`
- `docs/adr/`
- current implementation files for the chosen slice
- official docs only when protocol/library facts are unstable or external

## Ownership

Docs and design artifacts only.

## Do

- Define module ownership and non-ownership.
- Define public API inputs, outputs, errors, lifecycle rules, and state transitions.
- Define test obligations.
- Review the plan locally and with one independent pass.

## Do Not

- Ask low-level preference questions.
- Implement before decisions are recorded.

## Expected Output

- Accepted slice contract in docs/ADR/workflow result.

## Verification

- Plan review pass for coupling, encapsulation, responsiveness, and testability.
