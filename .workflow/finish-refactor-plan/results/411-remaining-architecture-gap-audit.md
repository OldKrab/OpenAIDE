# P411 remaining architecture gap audit

## Result

Selected `P412-remove-stale-settings-gap-wording` as the next packet.

## Findings

- The early architecture status still says the older full Settings snapshot remains as a fallback/presentation source, but the snapshot path has been removed.
- The Custom Agent replacement paragraph still frames launch-affecting replacement as a remaining gap and next selected packet, even though `agent/replaceCustom` implementation and cleanup metadata have landed.
- The Frontend Settings mutation paragraph still mentions legacy host fallback, which contradicts the typed-only cleanup work.

## Next

Update `docs/refactor-plan.md` so it accurately describes the current architecture state and leaves only real remaining gaps.
