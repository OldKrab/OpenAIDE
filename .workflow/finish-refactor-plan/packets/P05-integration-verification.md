# P05-integration-verification

## Objective

Prove the slice integrates with the repo and commit it.

## Context

This packet follows implementation and review fixes.

## Files / Sources

- touched implementation and docs
- package/Cargo metadata
- generated files
- workflow result notes

## Ownership

Verification, final docs cleanup, and commit only.

## Do

- Run required validation.
- Re-run flaky failures once serially before diagnosing.
- Update docs if implementation created or changed an architecture rule.
- Commit the slice with a neutral project-facing message.

## Do Not

- Start new feature work while fixing verification.

## Expected Output

- Verified commit.
- Result note with command evidence.

## Verification

- `npm run check`
- `npm test`
- affected builds
- additional slice-specific checks
