# P410 remove stale refactor-plan next step

## Result

Removed stale A4-era guidance from the living refactor plan.

## Implementation

- Replaced `Current Next Step` text with the active audit-driven workflow rule.
- Removed the contradictory `Remaining gap` from the completed Backend reveal handles section.
- Updated workflow state for the next audit packet.

## Verification

- `jq empty .workflow/finish-refactor-plan/state.json`
- `git diff --check`
