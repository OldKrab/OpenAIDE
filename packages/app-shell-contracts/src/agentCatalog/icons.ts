export type AgentIconId =
  | "openai"
  | "opencode"
  | "bot"
  | "code"
  | "terminal"
  | "sparkles"
  | "wrench"
  | "brain"
  | "cpu"
  | "zap"
  | "braces"
  | "blocks"
  | "cloud"
  | "database"
  | "flask"
  | "gauge"
  | "git-branch"
  | "globe"
  | "hammer"
  | "key"
  | "laptop"
  | "layers"
  | "network"
  | "rocket"
  | "search"
  | "shield";

export const agentIconIds = [
  "openai",
  "opencode",
  "bot",
  "code",
  "terminal",
  "sparkles",
  "wrench",
  "brain",
  "cpu",
  "zap",
  "braces",
  "blocks",
  "cloud",
  "database",
  "flask",
  "gauge",
  "git-branch",
  "globe",
  "hammer",
  "key",
  "laptop",
  "layers",
  "network",
  "rocket",
  "search",
  "shield",
] as const satisfies readonly AgentIconId[];

export function normalizedAgentIcon(value: unknown): AgentIconId | undefined {
  return agentIconIds.find((icon) => icon === value);
}

