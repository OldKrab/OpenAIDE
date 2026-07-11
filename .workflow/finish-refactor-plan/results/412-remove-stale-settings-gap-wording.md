# P412 remove stale Settings gap wording

## Result

Updated the refactor plan to match the current Settings and Custom Agent architecture.

## Implementation

- Removed the obsolete statement that the older full Settings snapshot remains as fallback/presentation state.
- Removed stale legacy host fallback wording for Frontend Custom Agent Settings mutations.
- Rewrote launch-affecting Custom Agent replacement as implemented `agent/replaceCustom` behavior instead of a remaining gap.
- Left the real remaining gap: future App Server-owned non-Agent Settings sections.

## Verification

- `jq empty .workflow/finish-refactor-plan/state.json`
- `git diff --check`
- Search for the stale wording targeted by this packet.
