# P59 ACP Probe/Auth Review Loop

Completed: 2026-06-27T03:55:04+03:00

## Review Passes

Ran `$doomsday-review` with three explorer subagents against `HEAD`.

Initial review findings:

- Requirements/tests found that the still-oversized `acp_runtime_kernel.rs` needed an
  explicit next oversized responsibility in workflow state.
- Code quality found that `acp_probe_auth.rs` imported validation predicates through
  the lifecycle facade instead of their owning capabilities module.
- Correctness reported no findings.

Fixes applied:

- Recorded ACP options-session lifecycle and retry as the next oversized
  `acp_runtime_kernel.rs` responsibility candidate in workflow state and
  `docs/refactor-plan.md`.
- Changed `acp_probe_auth.rs` to import validation directly from
  `acp_session_capabilities`.
- Removed the now-stale auth validation re-export from `acp_session_lifecycle.rs`
  and updated tests to import from the owning module.

Review rerun:

- Correctness: no findings.
- Requirements/tests: no findings.
- Code quality: no findings.
