# P29 ACP Prompt Runner Review Loop

Completed: 2026-06-27T03:06:00+03:00

## Review Passes

Ran `$doomsday-review` with three explorer subagents against `HEAD`:

- Correctness: no findings.
- Requirements/tests: no findings.
- Code quality: one finding.

## Finding Fixed

The code-quality pass found that prompt content validation was still split between
`AcpSessionClient::prompt` and the new prompt runner. The fix removed the early
prompt content build/validation from `AcpSessionClient` and removed prompt content
policy from `AcpSessionClient` and `AcpStartedSession`.

Startup context attachment validation remains in `AcpSessionWorker` because it is
session-start validation, not prompt-turn validation.

## Recheck

Ran a targeted `$doomsday-review` explorer recheck for correctness and code quality
after the fix. It reported no findings.
