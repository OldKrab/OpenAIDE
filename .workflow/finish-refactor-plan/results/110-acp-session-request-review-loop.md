# ACP Session Request I/O Split Review Loop

Ran `$doomsday-review` with independent correctness, requirements/tests, and
code-quality passes.

## Review Result

- Correctness: no findings.
- Requirements/tests: no findings.
- Code quality: no findings.

## Main-Agent Invariant Pass

Checked the lifecycle/request boundary after extraction:

- lifecycle still validates capabilities before load/list;
- lifecycle still attaches active sessions;
- lifecycle still owns load-replay setup/teardown and replay projection;
- lifecycle still maps request errors to product errors where it did before;
- request module returns raw `agent_client_protocol::Error`;
- request module retries `session/new`, `session/load`, and `session/list` once
  after successful ACP authentication;
- request module preserves new/load trace names and keeps list untraced.

No additional fixes were needed.
