# P56 Next Slice Selection

Completed: 2026-06-27T03:46:18+03:00

## Selected Slice

Split ACP probe and authentication execution out of
`agent/acp_runtime_kernel.rs`.

## Rationale

`agent/acp_runtime_kernel.rs` is still oversized at 731 lines and mixes several
responsibilities:

- public runtime facade methods;
- active session registry and session worker spawning;
- options-session lifecycle and retry;
- ACP probe execution;
- ACP authentication execution;
- low-level runtime/thread helpers.

Probe and authentication are the safest next cohesive boundary. They are public Agent
utility operations that open temporary ACP connections, perform initialize/auth
validation, handle host capability requests, and return normalized results. They do
not own active session registries, options-session state, config-option mutation,
Task session startup, or Native Session lifecycle.

## Non-Selection

Do not split options-session lifecycle or active session worker spawning in this
slice. Those remain in `AcpRuntimeKernel` until their own contracts are accepted.

Do not move session start/load/resume/close/delete behavior, config-option catalog
state, last-auth-method cache ownership, shutdown behavior, or runtime session
registry ownership in this slice.
