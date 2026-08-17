# Task Attention State and Shell-Local Notifications

Status: accepted

OpenAIDE represents notification-worthy Task changes as explicit App Server-owned Task Attention Events rather than asking clients to infer them from Task status or `unread`. The latest outstanding event has stable identity, reason, and occurrence time in authoritative Task state, which makes reconnect delivery and acknowledgement deterministic without turning historical unread Tasks into a notification backlog.

App Shells own whether and how an event becomes an OS notification. For the Web App this includes browser-profile opt-in, permission, focus, cross-tab deduplication, local delivery receipts, replacement, closing, and routing. This keeps browser-local capability facts out of App Server workflow state while preserving App Server ownership of the product decision that a Task needs attention.

The VS Code Extension uses VS Code's workbench Notification Center for every eligible Task Attention Event. It suppresses the alert when the affected Task was focused at the event occurrence time. The extension host owns focused-Task observation, extension-global delivery receipts, startup-baseline suppression, workbench notification presentation, and Task routing. VS Code's notification filter and Do Not Disturb controls remain the local enablement authority.

The Desktop App uses operating-system notifications for eligible Task Attention Events. It suppresses an alert only when the affected Task was focused at the event occurrence time; another focused Task does not suppress it. Desktop owns notification permission and enablement, startup-baseline suppression, local delivery receipts, one-current-notification-per-Task replacement, and click routing into the existing main window. Notification title and body contain only the Task title and a short product-authored reason, never prompt, Chat, Tool, permission, question, or error content.

The existing client-scoped `shell/showNotification` request is deliberately not used as the Task Attention lifecycle. Driving Task alerts through that request would require App Server to reason about browser-local permission, focus, and tab identity, while deriving alerts only in Frontend would make reconnect behavior ambiguous. The explicit attention-state seam costs one protocol and persistence concept but preserves one owner on each side of the App Server/App Shell boundary.
