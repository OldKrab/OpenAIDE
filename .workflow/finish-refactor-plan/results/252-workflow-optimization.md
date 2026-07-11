# Workflow Optimization

## Reason

The workflow reached `P231` because it recorded normal per-slice phases as separate
packets: selection, contract, implementation, review, verification, and next-slice
selection. Those packet ids are historical workflow identifiers, not product step counts,
but the granularity wasted context and made progress harder to read.

## Accepted Change

After `P231`, the workflow uses one packet per implementation slice. Each slice result
records:

- `contract`
- `implementation`
- `review`
- `verification`
- `commit`
- `next`

Separate packets or result files are reserved for real boundary replanning, blockers,
large deletes, broad renames, or other work that is not part of a normal slice loop.

## Review And Verification Policy

The workflow still requires local self-review and `$doomsday-review`, but the review loop
is bounded:

- Run one review pass after implementation.
- Fix accepted High and Medium findings.
- Re-review only the fixes and original risky areas.
- Run a third pass only for new material findings or disagreement.
- Stop on Low-only residuals after recording the decision.

During implementation, use targeted checks. Before commit, run the required full checks
for the slice. Save long logs outside the workflow and report only pass/fail plus failure
tails.

## Next

Continue from `P231-next-slice-selection`, but select the next implementation slice as a
single compact slice packet.
