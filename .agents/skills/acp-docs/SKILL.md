---
name: acp-docs
description: Find and summarize current Agent Client Protocol documentation from official ACP sources, including schema, tool calls, permissions, terminals, filesystem, sessions, and JSON-RPC flow. Use when the user mentions ACP, Agent Client Protocol, session/request_permission, tool calls, ACP schema, ACP JSON-RPC, agent/client integration, or needs protocol-accurate UI/runtime behavior.
---

# ACP Docs

## Quick Start

Run from the project root:

```bash
node .agents/skills/acp-docs/scripts/acp-docs.mjs search "session/request_permission tool calls"
node .agents/skills/acp-docs/scripts/acp-docs.mjs page protocol/tool-calls
node .agents/skills/acp-docs/scripts/acp-docs.mjs index permission
```

For saved OpenAIDE ACP trace logs:

```bash
node .agents/skills/acp-docs/scripts/acp-trace.mjs latest
node .agents/skills/acp-docs/scripts/acp-trace.mjs summary
node .agents/skills/acp-docs/scripts/acp-trace.mjs tools
node .agents/skills/acp-docs/scripts/acp-trace.mjs permissions
node .agents/skills/acp-docs/scripts/acp-trace.mjs raw-index
node .agents/skills/acp-docs/scripts/acp-trace.mjs show --line 110
```

`acp-trace.mjs` auto-finds the newest trace in `OPENAIDE_ACP_TRACE_DIR` or VS Code global storage. Pass a trace file or trace directory to inspect a specific run. It shortens long strings by default; use `--full` only when the user explicitly needs raw payload details.

Primary source is official ACP docs:

- `https://agentclientprotocol.com/llms.txt`
- `https://agentclientprotocol.com/protocol/schema.md`
- `https://agentclientprotocol.com/api-reference/openapi.json`

## Workflow

1. Start with `index <terms>` to find relevant pages.
2. Use `page <slug-or-url>` for exact docs. Prefer markdown URLs ending in `.md`.
3. Use `search <terms>` when the exact page is unknown.
4. If making protocol claims, cite the official URL used and state whether it came from live docs.
5. For OpenAIDE UI/runtime work, translate ACP payloads into normalized product terms. Do not expose raw ACP objects as frontend contract.
6. When checking OpenAIDE ACP trace logs, start with `acp-trace.mjs summary`, then `tools`, `permissions`, or `raw-index` as needed. Use `show --line N` for one event instead of dumping the full JSONL.

## Common Lookups

- Tool calls, kinds, statuses, content, permission: `page protocol/tool-calls`
- JSON-RPC flow and client/agent methods: `page protocol/overview`
- Content blocks: `page protocol/content`
- Terminal methods/output: `page protocol/terminals`
- File methods: `page protocol/file-system`
- Session lifecycle: `page protocol/session-setup`
- Config options: `page protocol/session-config-options`
- Full schema: `page protocol/schema`

## UI Mapping Notes

- Tool calls arrive through `session/update` with `sessionUpdate: "tool_call"` and later `tool_call_update`.
- Tool kinds include `read`, `edit`, `delete`, `move`, `search`, `execute`, `think`, `fetch`, and `other`.
- Tool statuses include `pending`, `in_progress`, `completed`, and `failed`.
- Permission is requested with `session/request_permission`; render it as a waiting permission block tied to the tool call.
- Permission options have `optionId`, `name`, and `kind`; kinds include `allow_once`, `allow_always`, `reject_once`, and `reject_always`.
- Tool content may include regular content, diffs, and terminal references. Keep parent tool identity stable when rendering nested content.

## Guardrails

- Bias to official `agentclientprotocol.com` docs over vendor-specific ACP pages.
- Check live docs for any unstable detail before implementation.
- Do not infer unsupported fields from examples.
- Do not use the unrelated "Agent Control Protocol" or "Agent Communication Protocol" when the project means Agent Client Protocol.
