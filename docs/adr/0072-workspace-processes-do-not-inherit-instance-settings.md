# Workspace Processes Do Not Inherit Instance Settings

Status: accepted

An Agent working on OpenAIDE itself starts a Web Shell or an App Server from its Task Workspace to try a change. That process inherited the environment of the App Server that started the Agent, including its state root. It found the running App Server there, and when its protocol version differed it asked that App Server to stop and replaced it. Every Task on that computer lost its App Server to a test run.

## Choice

- The App Server takes its own instance settings out of its process environment before it starts anything: `OPENAIDE_STORAGE_ROOT`, `OPENAIDE_RUNTIME_ROOT`, `OPENAIDE_APP_SERVER_PROTOCOL` and its legacy name, `OPENAIDE_PROJECT_ROOTS`, and the ACP diagnostics settings. Agents, Terminals ([ADR-0071](0071-terminals-are-shells-on-the-app-servers-computer.md)), git and npm therefore start without them.
- It passes them on only when it starts itself again, as the `remote` and `keep` commands do.
- The Web Shell does not pass its own settings to the App Server it starts: `OPENAIDE_WEB_*` and the App Server binary path.
- Any other variable, such as a test fixture's, still reaches child processes.

## Considered Options

- **Removing the settings at each spawn site.** Rejected: the ACP SDK starts Agents and can add variables but not remove them, and each new spawn site would need to remember.
- **Removing every `OPENAIDE_*` variable.** Rejected: fixtures and tools pass their own `OPENAIDE_*` settings to the Agents they start through the App Server.
- **Only changing the defaults when no state root is named.** Kept, but not enough: an inherited setting is a named state root.

## Consequences

- A command an Agent or Terminal runs that needs a particular instance must name it, for example through `OPENAIDE_WEB_ROLE`.
- App Server code reads its instance settings through `instance_env`, not `std::env`.
