# Settings Sections Are Named by the User's Goal

Status: accepted

Settings grew one tab per feature as features landed: Common, Data, Connection, Devices, Computers, a hidden Developer tab. The names described where the code lived, so a user who wanted to pair a phone, run Agents in WSL, or send logs had to guess which tab held it. This replaces that structure. It supersedes the layout paragraphs of [ADR-0021](0021-custom-agent-settings-ui.md); its storage and identity decisions stand.

## Choice

Settings has seven sections in one flat list: General, Agents, MCP servers, Skills, Worktrees, Remote, Troubleshooting. Each is named for what the user came to do. A section an App Shell cannot serve is left out rather than shown empty.

- **General** holds the choices about this app on this device: appearance, the send shortcut, notifications, where Agents run on a Windows Desktop, the phone's background work, and the app's own version and updates.
- **Remote** holds both directions of remote use on one page: the computer this shell is connected to, and the devices paired with that computer. The device list names its computer, because a phone or a browser tab on another machine is not "this computer".
- **Troubleshooting** starts with Export logs, since that is why most people open it. Reset task history and the developer tools follow. Developer tools are always visible; the unlock gesture is gone, because a hidden switch cannot be found by the person a maintainer asks to turn it on.

A resource with more than a row of state (an Agent, an MCP server, a Skill, a worktree) opens as its own page inside its section, with a way back to the list. Nothing is edited in a side pane.

Search covers every section at once and keeps the structure: sections and groups stay in place and rows that do not match drop out. It also matches the explanations, so a user can find a setting by what it does.

An explanation that most users do not need sits behind an info button next to the thing it explains. It never takes space on the page and never carries something required to finish the task.

On a constrained width Settings shows the section list or one section, never both. A route that names a section opens that section.

## Mechanism

- Each section is a declaration: groups of rows, each with a label, keywords, an optional explanation, and its control. The page, the search, and the explanations all read that one declaration, so a setting cannot be on the page but missing from search.
- Section identifiers are part of the App Shell contract. The removed identifiers are accepted in a route and resolved to the section that now holds their content, so an old link or command still lands somewhere useful.
- The Troubleshooting developer group is shown from the App Server's runtime settings read. If that read fails the group is left out and the other sections still load.

## Considered Options

- **Two levels of navigation, sections under headings.** Rejected: seven entries do not need grouping, and a heading is one more name to interpret.
- **A "Runs on" section for both WSL and remote computers.** Rejected: someone connecting to another computer does not think of it as choosing where Agents run, and someone picking WSL on first launch does not look under remote access. The two stay where each goal leads.
- **Search within the open section only.** Rejected: the user searching does not yet know which section holds the setting.
- **Keeping developer tools behind an unlock.** Rejected for the reason above; the tools change nothing until used.
