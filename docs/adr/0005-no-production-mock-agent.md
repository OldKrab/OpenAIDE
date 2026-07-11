# No Production Mock Agent

OpenAIDE production runtime will run real ACP Agents only. The first built-in Agent is Codex through `codex-acp`; if it cannot launch, initialize, or authenticate, OpenAIDE shows a setup or auth failure instead of falling back to mock behavior. Mock Agents may remain only for tests or explicit development fixtures, never as a production fallback.
