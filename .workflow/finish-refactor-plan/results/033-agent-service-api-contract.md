# P12 Agent Service API Contract

Completed: 2026-06-27T02:33:52+03:00

## Accepted Shape

`AgentService` is an internal backend service for Agent-facing utility operations:

- `probe(params: AgentProbeParams) -> Result<AgentProbeResult, RuntimeError>`
- `authenticate(params: AgentAuthenticateParams) -> Result<AgentAuthenticateResult, RuntimeError>`
- `list_sessions(params: AgentListSessionsParams) -> Result<AgentListSessionsResult, RuntimeError>`
- `config_options(params: AgentConfigOptionsParams) -> Result<ConfigOptionsCatalog, RuntimeError>`
- `set_config_option(params: SessionSetConfigOptionParams) -> Result<ConfigOptionsCatalog, RuntimeError>`

`TaskService` keeps its current public facade methods and delegates to `AgentService`:

- `probe_agent`
- `authenticate_agent`
- `list_agent_sessions`
- `config_options`
- `set_config_option`

## Ownership

- `AgentService` owns `AgentRegistry` validation for these public Agent operations.
- `AgentService` owns simple request validation needed before calling `AgentGateway`,
  such as non-empty auth method and absolute workspace root for session listing.
- `AgentGateway` remains the low-level `AgentRuntime` adapter and does not grow
  product validation rules.
- `TaskTurnLifecycle` remains the owner of Task creation, prompt execution, cancel,
  permission response, recovery, and live Native Session lifecycle policy.
- Agent session start/load/resume/close stay available through `AgentGateway` for Task
  workflows; they are not part of this public utility `AgentService` split.

## Non-Goals

- No protocol method rename.
- No transport dispatcher change beyond continuing to call `TaskService` facade
  methods.
- No durable storage writes.
- No state-sync or notification changes.
- No cache of Agent options.

## Review And Test Requirements

- Existing runtime contract tests for Agent probe/auth/session list/config options must
  keep passing.
- Add or update a boundary test so `TaskService` does not re-own direct
  `Agent*Request` construction for public Agent utility methods after the split.
- Keep production Rust source files below the project line limit.
