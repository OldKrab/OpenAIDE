# P324 Active ACP Secret Resolver Injection

## Scope

Wired active ACP session startup to accept an injected Agent secret resolver and
made responsive Task preparation provide a typed task-scoped `secret/read`
resolver for Agent `secret_env`.

## Decisions

- `AgentSessionStart` and `AgentSessionLoad` carry an optional
  `AgentSecretResolver`.
- Active ACP sessions pass the resolver to `AcpAgentConfig` when building the
  process environment.
- Probe, auth, and options sessions still use the legacy host secret bridge
  because they are not Task-scoped UI surfaces.
- Task preparation resolves custom Agent secret env names via existing secret
  keys: `openaide.agent.{agentId}.env.{name}`.
- Known follow-up: add protocol-edge delivery for task-scoped server requests
  opened while a Task responder is already subscribed. This keeps the current
  typed secret path scoped to the normal create-then-subscribe preparation flow.

## Verification

- `cargo fmt --all --check`
- `cargo test -p openaide-runtime start_session_passes_resolved_secret_env_to_acp_process -- --nocapture`
- `cargo test -p openaide-runtime preparation_secret_request -- --nocapture`

## Next

Add the App Server Protocol edge hook that delivers task-scoped server-request
envelopes immediately when a request opens and an eligible Task responder is
already subscribed.
