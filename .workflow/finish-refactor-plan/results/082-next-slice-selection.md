# P61 Next Slice Selection

Completed: 2026-06-27T03:57:06+03:00

## Selected Slice

Split ACP options-session lifecycle and retry out of
`agent/acp_runtime_kernel.rs`.

## Rationale

`agent/acp_runtime_kernel.rs` remains oversized at 564 lines after the probe/auth
split. The largest cohesive remaining responsibility inside it is options-session
management:

- active options-session cache keyed by normalized cwd and Agent request key;
- generation assignment;
- stale options-session invalidation;
- retry-on-`NotReady` behavior;
- worker thread spawning and startup timeout handling;
- shutdown close task extraction for the active options session.

This is distinct from `agent/acp_options_session.rs`, which already owns the live
options worker protocol and command loop. Moving the manager layer next leaves
`AcpRuntimeKernel` closer to a facade over public Agent runtime operations and active
Task session registry behavior.

## Non-Selection

Do not move active Task session start/load/resume/close/delete, session registry
ownership, prompt/cancel dispatch, probe/auth execution, or public runtime facade
method names in this slice.

Do not change config option catalog behavior, session list behavior, retry policy,
startup timeout text, invalidation behavior, auth method cache semantics, or shutdown
close ordering.
