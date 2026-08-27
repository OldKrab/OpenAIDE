# Upstream codex-acp subagent sessions

Research date: 2026-08-27

## Executive summary

Upstream `agentclientprotocol/codex-acp` now has two compatible representations of Codex subagents:

- Since `v1.1.4`, Codex collaboration and subagent activity can be projected into ordinary ACP Tool calls. Codex-specific identity and activity are carried in namespaced `_meta.codex` metadata. This is the representation OpenAIDE's accepted Task Chat design currently understands.
- `v1.7.0`, released on 2026-08-27, adds negotiated native ACP subagent sessions. A capable client can receive a tree of restricted child sessions, with each child's messages, thoughts, plans, tools, permissions, elicitations, and terminal state routed independently. A client that does not negotiate this draft capability continues to receive the legacy Tool-call representation.

The native contract is not stable ACP yet. It implements the still-open ACP [subagent RFD PR #1992](https://github.com/agentclientprotocol/agent-client-protocol/pull/1992), and codex-acp temporarily defines the missing wire types itself because released ACP SDKs do not contain them. OpenAIDE should therefore treat this as an opt-in adapter extension, not as a protocol feature it receives automatically by updating a package.

## Timeline

### 2026-07-15: visible legacy Tool calls

[Commit `b7466f5`](https://github.com/agentclientprotocol/codex-acp/commit/b7466f5785e56306e532f998bfe3235e26ea0470), released in `v1.1.4`, made previously hidden Codex subagent activity visible using standard ACP Tool calls:

- `subAgentActivity` start/completion events become `tool_call` and `tool_call_update` events with titles such as `Start subagent weather_research`.
- The Tool's `rawInput` carries `agentThreadId`, `agentPath`, and `activityKind`.
- `_meta.codex.subagent` carries `threadId`, `path`, and `activity`.
- Collaboration Tool calls preserve their sender, receivers, model, reasoning effort, and state, with identity also duplicated into `_meta.codex.collaboration`.
- `session/load` replays the same ordinary Tool-call representation.

This stage improved visibility but remained a flattened parent transcript. A client could identify a worker and its activity, but ACP still supplied no independent stream for that worker's messages, plans, nested Tools, or nested children.

### 2026-08-27: negotiated native child sessions

[codex-acp PR #419](https://github.com/agentclientprotocol/codex-acp/pull/419) merged as [commit `6067b7f`](https://github.com/agentclientprotocol/codex-acp/commit/6067b7f48fe37db82b6ddb9d596a4a4d8cb8a2e4) and shipped in [release `v1.7.0`](https://github.com/agentclientprotocol/codex-acp/releases/tag/v1.7.0) on 2026-08-27. It adds:

- bilateral capability negotiation during `initialize`;
- `subagent_spawned` updates on the immediate parent before any child output;
- independent `session/update` streams addressed by the child session ID;
- nested routing by announcing grandchildren on their immediate parent;
- exactly-once terminal `subagent_state_update` events;
- child-aware routing for Tools, permission requests, and elicitations;
- reconstruction of the child tree during `session/load`;
- lifecycle isolation so child turns do not end the root prompt;
- a legacy Tool-call fallback when the client has not negotiated native sessions.

The PR reports 447 passing and 28 skipped tests. Its implementation buffers child traffic until Codex announces identity, scopes permission correlation by thread for concurrent children, keeps the parent prompt open until announced children finish, and gives a resumed/reopened worker a new lifecycle generation rather than reviving a terminal child identity.

## Native wire contract

The authoritative proposed semantics are in ACP [PR #1992](https://github.com/agentclientprotocol/agent-client-protocol/pull/1992); codex-acp documents its implementation in [`docs/subagent-sessions.md`](https://github.com/agentclientprotocol/codex-acp/blob/v1.7.0/docs/subagent-sessions.md).

Negotiation is deliberately bilateral:

```json
{
  "clientCapabilities": {
    "subagents": {}
  }
}
```

The agent responds with `agentCapabilities.sessionCapabilities.subagents: {}`. codex-acp must not emit native child updates unless both sides opt in. Because released SDKs may strip the draft canonical field, codex-acp also accepts JetBrains AIR's `nativeSubagentSessions` capability in `_meta.jetbrains.air.capabilities`; new clients are expected to prefer the canonical field.

For each announced worker, the parent receives:

- `subagent_spawned`, containing required `subagentSessionId`, human-readable `name`, delegated `task`, and per-child capabilities;
- ordinary ACP updates whose outer `sessionId` is the child's ID;
- one terminal `subagent_state_update` on the immediate parent, with `completed`, `failed`, `cancelled`, or `disconnected`.

The child is a restricted observational session, not a normal user-created session. The client must not call `session/new`, `load`, `resume`, `fork`, `prompt`, queueing, or steering methods with a child ID. The draft allows targeted `session/cancel` or `session/close` only when the individual child advertises those flags. codex-acp `v1.7.0` advertises an empty child capability object, so it supports neither targeted operation today.

Child sessions inherit the parent's effective workspace, additional directories, MCP servers, and client capabilities. They may still read and edit files, run commands, and request permissions: “observational” restricts client-to-child conversation, not workspace effects or security boundaries. Parent and child updates can interleave, and ACP defines ordering only within each session stream, not globally between sessions.

In ACP v1, every announced child must terminate before the parent `session/prompt` returns. Detached background workers are out of scope. codex-acp waits for outstanding children and marks them failed after its ten-minute safety timeout.

## Replay and lifecycle caveats

- `session/load` is the authoritative reconstruction path. codex-acp rebuilds the child tree from Codex history and reports a history orphan as `disconnected` when no outcome can be proven.
- `session/resume` does not replay or reconstruct children. A client that needs an authoritative historical tree must load the parent.
- The draft requires `disconnected` when the outcome is unknown. codex-acp currently maps live timeout, `shutdown`, and `notFound` fallbacks to `failed`; its own documentation calls out this divergence.
- The Codex aggregate app-server events do not expose collaborator nickname or role, so the adapter derives a name from the activity path and otherwise uses a stable synthetic name.
- The underlying Codex app-server V2 does not expose an independent background-task lifecycle with stable task ID, progress, and terminal state. PR #419 therefore does not advertise or synthesize AIR `asyncTasks`.
- The standard ACP TypeScript SDK had not published the PR #1992 types at release time. codex-acp isolates temporary structural types in [`AcpSubagents.ts`](https://github.com/agentclientprotocol/codex-acp/blob/v1.7.0/src/subagents/AcpSubagents.ts).
- PR #1992 remains open as of this research date, so field names and lifecycle rules may still change before ACP stabilization.

## OpenAIDE implications

There is no immediate behavior change in the OpenAIDE built-in. OpenAIDE pins its own fork, `@openaide/codex-acp@1.0.0`, and its ACP client does not currently advertise `clientCapabilities.subagents`. Even if the upstream `v1.7.0` adapter were run as a Custom Agent, bilateral negotiation should keep it on the legacy Tool-call path.

That fallback aligns with the accepted OpenAIDE Task Chat specification: every Codex collaboration/activity Tool remains a separate transcript row, `_meta.codex.subagent` is interpreted only at the Codex integration boundary, and no child-thread navigation or orchestration controls are exposed. The July representation is therefore a compatibility floor worth retaining in the OpenAIDE fork.

Native adoption would be a product and protocol change, not only a dependency bump. It would require at least:

1. Porting upstream PR #419 into the OpenAIDE fork or changing the product-controlled adapter/package policy.
2. Adding draft subagent variants at OpenAIDE's single ACP schema boundary and advertising the client capability only after the full path understands them.
3. Making App Server own the parent/child session tree, per-session ordering, terminal lifecycle, and load reconstruction. Frontend should receive normalized product state rather than raw draft ACP objects.
4. Routing child messages, thoughts, plans, Tools, permissions, and elicitations by child session ID while preserving one connection-level security and permission boundary.
5. Deciding the shared Web/VS Code UI: where child activity appears, whether selection changes the transcript, how nesting and concurrent progress render, and how historical `disconnected` children are presented.
6. Preserving the legacy Tool-call path for unnegotiated adapters and avoiding duplicate rows when native updates are negotiated.
7. Testing real live and replay boundaries: spawn-before-output, nested interleaving, concurrent permissions, cancellation of the root, load with orphaned children, resume without replay, reconnect, timeout, and downgrade fallback.

The current specification explicitly says OpenAIDE exposes no thread navigation or orchestration controls until a Codex integration supplies that capability. PR #419 now supplies an observational child-session capability, but not targeted cancel/close or client prompting. Product agreement is needed on whether independent inspection alone justifies changing the accepted transcript model.

## Primary sources

- [codex-acp `v1.7.0` release](https://github.com/agentclientprotocol/codex-acp/releases/tag/v1.7.0)
- [codex-acp PR #419: native ACP subagent sessions](https://github.com/agentclientprotocol/codex-acp/pull/419)
- [Native implementation commit `6067b7f`](https://github.com/agentclientprotocol/codex-acp/commit/6067b7f48fe37db82b6ddb9d596a4a4d8cb8a2e4)
- [codex-acp native-subagent documentation at `v1.7.0`](https://github.com/agentclientprotocol/codex-acp/blob/v1.7.0/docs/subagent-sessions.md)
- [Temporary draft wire types at `v1.7.0`](https://github.com/agentclientprotocol/codex-acp/blob/v1.7.0/src/subagents/AcpSubagents.ts)
- [ACP PR #1992: proposed Subagent Sessions RFD](https://github.com/agentclientprotocol/agent-client-protocol/pull/1992)
- [Legacy Tool-call commit `b7466f5`](https://github.com/agentclientprotocol/codex-acp/commit/b7466f5785e56306e532f998bfe3235e26ea0470)
