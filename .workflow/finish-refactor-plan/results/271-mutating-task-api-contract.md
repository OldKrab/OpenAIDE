# A3b Mutating Task API Contract

## Contract

A3b implements `task/create` and `task/send` without preserving the old
create-with-first-prompt behavior.

### Dependency

The current App Server Protocol `TaskCreateParams` contains `projectId` and
`agentId`, not `workspaceRoot`. That is correct for the target architecture, but
the current runtime can only start or prepare Agent work from a workspace root.
Therefore A3b first needs a minimal Backend-owned Project resolver.

The Project resolver must:

- Map a known `ProjectId` to Task creation context, including workspace root,
  safe project label, isolation default, and allowed-root metadata needed by
  existing runtime code.
- Be Backend-owned and App Server internal.
- Avoid shell-specific state, Frontend state, raw path protocol fields,
  temporary host URLs, or reverse lookup from opaque hash ids.
- Be replaceable by the full Projects module in A8 without changing the public
  Task API.

### `task/create`

- Creates a durable Task for an existing Project and Agent.
- Never sends a prompt, appends a user Chat item, creates a turn, or calls the
  old prompt-start path.
- Returns a renderable `TaskSnapshot` with explicit preparation state.
- May start Native Session preparation asynchronously only after the durable Task
  exists.
- Publishes committed Task Navigation or Task state after durable acceptance.

### `task/send`

- Is the first method allowed to commit a user message and start an Agent turn.
- Validates idempotency key, request fingerprint, task revision, active turn,
  empty message, attachments, Agent readiness, and required options.
- Commits the user message and active turn atomically before spawning Agent work.
- Returns committed `turnId`, `userMessageId`, and a Task snapshot.
- Reuses an existing committed turn for the same idempotency key and request
  fingerprint; rejects conflicts.
- Publishes ordered state-sync events after durable acceptance.

## Status

Contract recorded; implementation is next.

## Review Notes

The old runtime paths are intentionally not a direct protocol implementation:

- `TaskTurnLifecycle::create_prompt_start` starts an Agent session, creates a
  Task, appends a user message, appends a running turn, and spawns Agent work in
  one operation.
- `TaskTurnLifecycle::prompt` starts or resumes a Native Session and appends the
  user message in the same request.

A3b may reuse internal pieces from those paths, but the App Server Protocol
handlers must expose the split product workflow above.

## Next

Implement the minimal Project resolver required by `task/create`.
