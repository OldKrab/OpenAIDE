import type { InfoText } from "./settingsModel";

/**
 * Every explanation Settings offers. They sit behind info affordances so the
 * pages stay sparse, and the search reads them so a question still finds its setting.
 */
export const settingsInfo = {
  runtime: {
    title: "Windows or WSL",
    body: [
      "Agents use the projects, tools and sign-ins of the side you pick. Windows and WSL each keep their own.",
      "OpenAIDE restarts to switch. Running tasks stop first.",
    ],
  },
  background: {
    title: "Background work",
    body: [
      "Agents on this phone run in Termux. Android pauses apps it considers idle, so work can stop when the screen locks.",
      "Unrestricted battery use for OpenAIDE and Termux keeps work running. A notification shows while it runs and can stop it.",
    ],
  },
  agents: {
    title: "Agents",
    body: [
      "An Agent is an external program that does the work of a Task. Built-in Agents are set up for you; a Custom Agent is any program that speaks ACP.",
      "Connected means it started, answered and is signed in.",
    ],
  },
  command: {
    title: "Command",
    body: [
      "The command runs directly, not through a shell, so pipes, globs and shell variables do nothing here.",
      "Changing the command or its environment replaces the Agent with a new one for future tasks. Earlier tasks keep their history.",
    ],
  },
  mcp: {
    title: "MCP servers",
    body: ["A server gives compatible Agents extra tools and data. Global servers reach every project; a project's servers reach only its tasks."],
  },
  skills: {
    title: "Skills",
    body: [
      "A Skill is a set of instructions an Agent loads when a task matches it. Global Skills apply everywhere; a project's Skills apply to its tasks.",
      "Skills are files on disk. Edit them there; OpenAIDE shows what Agents will read.",
    ],
  },
  worktrees: {
    title: "Worktrees",
    body: [
      "A worktree is a second checkout of a project on its own branch, so tasks can work in parallel without touching each other's files.",
      "Managed worktrees are created and removed by OpenAIDE. External ones were made outside it and are only listed here.",
    ],
  },
  connectedTo: {
    title: "Connected to",
    body: [
      "Projects, tasks and history live on the computer where agents work. Switching shows that computer's work; nothing is moved.",
      "OpenAIDE restarts to switch.",
    ],
  },
  devices: {
    title: "How devices connect",
    body: [
      "Each device has its own key. Pairing makes this computer trust that key; there is no account or password.",
      "The connection is encrypted end to end. It goes directly when the networks allow it, otherwise through public relays run by the iroh project, which see addresses and keys, never content.",
    ],
  },
  exportLogs: {
    title: "Diagnostics",
    body: [
      "The export holds logs and state with prompts, paths and secrets redacted.",
      "Session history and raw traces are added only if you select them, behind a warning.",
    ],
  },
  reset: {
    title: "Reset task history",
    body: [
      "Deletes every Task, chat, tool detail and recalled prompt on this computer.",
      "Projects, files, worktrees, Agents, MCP servers, preferences and sign-ins are kept. Sessions the Agents own may appear again in history.",
    ],
  },
} satisfies Record<string, InfoText>;
