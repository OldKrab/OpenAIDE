# Workspace-First External Session Discovery

OpenAIDE will discover external Native Sessions workspace-first in the first iteration: when an Agent supports `session/list`, the runtime requests sessions for the active workspace root and presents only those as adoption candidates. This keeps task navigation minimal, avoids cross-project noise, and still supports ACP adoption for the project the user is working in; broader Agent-wide browsing can be added later if there is a clear workflow.
