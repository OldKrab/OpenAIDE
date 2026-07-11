# P04-review-loop

## Objective

Review the implementation repeatedly until it is good enough to commit.

## Context

Use doomsday-style review standards for correctness, requirements/tests, and code quality.

## Files / Sources

- slice diff
- slice plan/ADR
- relevant tests
- project rules

## Ownership

Review artifacts and current-slice fixes only.

## Do

- Run local review first.
- Use subagents for bounded independent review when available.
- Verify every accepted finding against local files.
- Fix High/Medium findings and repeat.
- Stop according to the bounded review controller in `plan.md`; Low-only residual risk
  does not reopen the loop.

## Do Not

- Treat raw subagent output as final truth.
- Keep reviewing forever after repeated passes find no material issues.

## Expected Output

- Result note with accepted/rejected findings, review round count, and stop decision.

## Verification

- Review output must include exact file/line proof for findings.
