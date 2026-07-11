# P03-implementation-slice

## Objective

Implement the accepted slice exactly.

## Context

Implementation starts only after `P02` records the API/ownership contract.

## Files / Sources

Named by the accepted slice contract.

## Ownership

Only files assigned to the slice. Other dirty files must be classified before editing.

## Do

- Implement deep, narrow modules.
- Keep edge handlers thin.
- Add focused behavior tests.
- Regenerate TypeScript protocol bindings when Rust protocol changes.
- Keep production files under source-size limits.

## Do Not

- Preserve legacy code for compatibility.
- Add product workflow decisions to Frontend or shell code.
- Add catch-all services or generic managers.

## Expected Output

- Implemented slice ready for review.

## Verification

- Narrow tests first, then root checks as required by blast radius.
