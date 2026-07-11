# P421 - Update Refactor Plan Status

## Result

Updated stale `docs/refactor-plan.md` status text after the fast Settings and shell-product cleanup packets.

## Changes

- Replaced stale non-Agent Settings projection gap wording with current implemented status.
- Updated Custom Agent shell-stub wording to reflect deletion rather than failing shell mutations.
- Removed stale "future MCP/Skills" and "runtime routing remains next" statements.
- Clarified remaining shell-local routing language after removing shell-provided Agent bootstrap.

## Verification

- `rg` for removed stale phrases
- `git diff --check`

## Next

P422 should fast-pick the next concrete code gap; if no small stale shell/product boundary remains, move to the next accepted internal refactor slice.
