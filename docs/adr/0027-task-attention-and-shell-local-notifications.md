# Task Attention State and Shell-Local Notifications

Status: accepted

OpenAIDE represents notification-worthy Task changes as explicit App Server-owned Task Attention Events rather than asking clients to infer them from Task status or `unread`. The latest outstanding event has stable identity, reason, and occurrence time in authoritative Task state, which makes reconnect delivery and acknowledgement deterministic without turning historical unread Tasks into a notification backlog.

App Shells own whether and how an event becomes an OS notification. For the Web App this includes browser-profile opt-in, permission, focus, cross-tab deduplication, local delivery receipts, replacement, closing, and routing. This keeps browser-local capability facts out of App Server workflow state while preserving App Server ownership of the product decision that a Task needs attention.

The VS Code Extension presents eligible events through VS Code's workbench Notification Center rather than an operating-system notification API. Its extension host owns focused-Task and VS Code-window observation, extension-global delivery receipts, startup-baseline suppression, and Task routing. An event is eligible whenever its Task was not focused at occurrence, so work completing in another Task remains visible without interrupting the Task already being viewed. VS Code's notification filter and Do Not Disturb controls are the local enablement authority, so OpenAIDE does not duplicate the Web App's browser-permission setting in this shell.

The existing client-scoped `shell/showNotification` request is deliberately not used as the Task Attention lifecycle. Driving Task alerts through that request would require App Server to reason about browser-local permission, focus, and tab identity, while deriving alerts only in Frontend would make reconnect behavior ambiguous. The explicit attention-state seam costs one protocol and persistence concept but preserves one owner on each side of the App Server/App Shell boundary.
