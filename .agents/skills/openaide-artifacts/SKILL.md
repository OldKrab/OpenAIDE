---
name: openaide-artifacts
description: Diagnose saved OpenAIDE app artifacts and diagnostics, including App Server task stores, task.json, message_meta.json, messages.jsonl, diagnostics/logs JSONL files, ACP traces, and persisted OpenAIDE agent settings. Use when investigating OpenAIDE session history, broken chat rendering, persisted task state, custom/built-in agent setup, App Server artifacts, extension/App Server failures, or user screenshots that likely come from saved OpenAIDE data.
---

# OpenAIDE Artifacts

Diagnose artifacts created by the OpenAIDE app on this computer. This skill is for app-generated App Server data, not arbitrary VS Code logs and not Codex's own session logs.

## Quick Start

```bash
node .agents/skills/openaide-artifacts/scripts/oa-artifacts.mjs roots
node .agents/skills/openaide-artifacts/scripts/oa-artifacts.mjs logs --limit 80
node .agents/skills/openaide-artifacts/scripts/oa-artifacts.mjs failures --limit 40
node .agents/skills/openaide-artifacts/scripts/oa-artifacts.mjs agents
node .agents/skills/openaide-artifacts/scripts/oa-artifacts.mjs tasks
node .agents/skills/openaide-artifacts/scripts/oa-artifacts.mjs doctor
node .agents/skills/openaide-artifacts/scripts/oa-artifacts.mjs messages <task-id-or-prefix>
node .agents/skills/openaide-artifacts/scripts/oa-artifacts.mjs search "Working"
```

Use `--root <path>` or `OPENAIDE_ARTIFACT_ROOT=<path>` when the app storage root is not auto-detected. The root may be either the extension storage directory or its `runtime` directory.

## Workflow

1. Locate artifact roots with `roots`.
2. For App Server/setup bugs, start with `failures`, then `logs runtime --grep <agent-or-error>` or `logs extension --grep <action>`.
3. For custom Agent bugs, run `agents` to compare persisted `openaide.agents` with what the UI shows.
4. List candidate tasks with `tasks`, filtered by title/status if needed.
5. Run `doctor` globally, then on the suspicious task id.
6. Read `messages <task>` for the persisted chat sequence.
7. Use `search <text>` when chasing a phrase from a screenshot, tool title, error, or response fragment.
8. If reporting a bug, use `export <task> --out <file>` to create a compact redacted report.

## What To Check First

- App diagnostics live under the OpenAIDE App Server storage root, not scattered VS Code log folders.
- File logs are in `diagnostics/logs/`: check `openaide-extension.jsonl` and `openaide-app-server.jsonl`.
- ACP protocol traces are in `diagnostics/acp-traces/` when enabled.
- Settings actions appear in the extension log as webview action `type` values such as `settings.snapshot`, `agent.custom.save`, `agent.configOptions`.
- App Server failures appear in the App Server log as `rpc_request_failed`, usually with `fields.method` and `fields.error`.
- Custom Agent command failures often mean the command is not available on the App Server PATH; built-in policies may have fallbacks that custom commands do not.
- Probe timeouts usually mean the ACP process started but did not answer initialize/configuration in time.
- `agents` reads OpenAIDE settings and VS Code settings history, not generic VS Code logs.
- Fragmented assistant output: many adjacent `agent_text` rows with tiny chunks.
- Boilerplate activity rows: `Working` plus `Started` rows that should not dominate Chat.
- Tool activity without useful preview or grouped title.
- Broken store invariants: missing `task.json`, invalid JSONL, duplicate identities, meta/message count mismatch.
- Timestamp problems: non-ISO or unparsable task/message timestamps.

## Rules

- Default commands are read-only.
- Do not inspect generic VS Code logs unless the user explicitly widens scope.
- Do not inspect `~/.codex/sessions` for this skill; those are Codex artifacts, not OpenAIDE app artifacts.
- Prefer `--json` when another script or test will consume the output.
- Redact absolute home paths when exporting reports unless the user explicitly asks for raw paths.
