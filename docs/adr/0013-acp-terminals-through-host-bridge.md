# ACP Terminals Through Host Bridge

OpenAIDE will advertise ACP terminal capability only when the current Host can create, stream, wait, kill, and release terminal commands through a Host-owned terminal bridge. Terminal output is persisted as Chat activity and remains visible after release, while logs and support export keep only redacted metadata; the runtime must not implement ACP terminals as untracked hidden child processes.
