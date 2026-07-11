# P398 Remaining Architecture Gap Audit

## Result

- Confirmed the shell product-bridge cleanup track is closed for active source.
- Reviewed the active top-level refactor plan for remaining architecture gaps.
- Selected the next real product gap: launch-affecting Custom Agent edits need a distinct App Server workflow that creates a new Agent identity, warns the user, and cleans old local overlays/history instead of generic save semantics.

## Next Packet

P399 should grill and record the App Server Protocol/API for launch-affecting Custom Agent replacement before implementation.

Key API questions to answer in P399:

- Which Custom Agent fields are launch-affecting versus metadata-only.
- What warning/confirmation shape Frontend receives before replacement.
- What Backend cleanup is required for old Agent identity, overlays, settings rows, cached status, and task/session history.
- How old tasks render after their Agent identity is replaced or deleted.
- Whether replacement is one method or a two-step prepare/confirm workflow.

## Verification

- `git status --short`
- `rg -n "Current progress|Remaining|remaining|TODO|next|legacy|fallback|App Server|shell|bridge|should|not yet|gap" docs/refactor-plan.md`
- Manual read of active plan sections 1-9 and A6 status.
