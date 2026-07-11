import {
  Blocks,
  Bot,
  Brain,
  Braces,
  Cloud,
  Code2,
  Cpu,
  Database,
  FlaskConical,
  Gauge,
  GitBranch,
  Globe,
  Hammer,
  KeyRound,
  Laptop,
  Layers,
  Network,
  Rocket,
  Search,
  Shield,
  Sparkles,
  Terminal,
  Wrench,
  Zap,
} from "lucide-react";
import type { AgentIconId } from "@openaide/app-shell-contracts";

const lucideAgentIcons = {
  bot: Bot,
  code: Code2,
  terminal: Terminal,
  sparkles: Sparkles,
  wrench: Wrench,
  brain: Brain,
  cpu: Cpu,
  zap: Zap,
  braces: Braces,
  blocks: Blocks,
  cloud: Cloud,
  database: Database,
  flask: FlaskConical,
  gauge: Gauge,
  "git-branch": GitBranch,
  globe: Globe,
  hammer: Hammer,
  key: KeyRound,
  laptop: Laptop,
  layers: Layers,
  network: Network,
  rocket: Rocket,
  search: Search,
  shield: Shield,
} satisfies Partial<Record<AgentIconId, typeof Bot>>;

export const agentIconLabels = {
  openai: "OpenAI",
  opencode: "OpenCode",
  bot: "Bot",
  code: "Code",
  terminal: "Terminal",
  sparkles: "Sparkles",
  wrench: "Tools",
  brain: "Reasoning",
  cpu: "Runtime",
  zap: "Fast",
  braces: "Braces",
  blocks: "Blocks",
  cloud: "Cloud",
  database: "Database",
  flask: "Experiment",
  gauge: "Gauge",
  "git-branch": "Branch",
  globe: "Web",
  hammer: "Build",
  key: "Key",
  laptop: "Computer",
  layers: "Layers",
  network: "Network",
  rocket: "Launch",
  search: "Search",
  shield: "Shield",
} satisfies Record<AgentIconId, string>;

export function AgentIcon({ agentId, agentName, icon = "bot", size }: { agentId?: string; agentName?: string; icon?: AgentIconId; size: number }) {
  if (icon === "openai" || agentId === "codex" || agentName?.trim().toLowerCase() === "codex") {
    return <OpenAiIcon size={size} />;
  }
  if (icon === "opencode" || agentId === "opencode" || agentName?.trim().toLowerCase() === "opencode") {
    return <OpenCodeIcon size={size} />;
  }
  const Icon = lucideAgentIcons[icon] ?? Bot;
  return <Icon size={size} aria-hidden="true" />;
}

function OpenAiIcon({ size }: { size: number }) {
  return (
    <svg
      aria-hidden="true"
      className="agent-brand-icon openai-agent-icon"
      fill="currentColor"
      height={size}
      role="img"
      viewBox="0 0 24 24"
      width={size}
    >
      <path d="M22.2819 9.8211a5.9847 5.9847 0 00-.5157-4.9108 6.0462 6.0462 0 00-6.5098-2.9A6.0651 6.0651 0 004.9807 4.1818a5.9847 5.9847 0 00-3.9977 2.9 6.0462 6.0462 0 00.7427 7.0966 5.98 5.98 0 00.511 4.9107 6.051 6.051 0 006.5146 2.9001A5.9847 5.9847 0 0013.2599 24a6.0557 6.0557 0 005.7718-4.2058 5.9894 5.9894 0 003.9977-2.9001 6.0557 6.0557 0 00-.7475-7.0729zm-9.022 12.6081a4.4755 4.4755 0 01-2.8764-1.0408l.1419-.0804 4.7783-2.7582a.7948.7948 0 00.3927-.6813v-6.7369l2.02 1.1686a.071.071 0 01.038.052v5.5826a4.504 4.504 0 01-4.4945 4.4944zm-9.6607-4.1254a4.4708 4.4708 0 01-.5346-3.0137l.142.0852 4.783 2.7582a.7712.7712 0 00.7806 0l5.8428-3.3685v2.3324a.0804.0804 0 01-.0332.0615L9.74 19.9502a4.4992 4.4992 0 01-6.1408-1.6464zM2.3408 7.8956a4.485 4.485 0 012.3655-1.9728V11.6a.7664.7664 0 00.3879.6765l5.8144 3.3543-2.0201 1.1685a.0757.0757 0 01-.071 0l-4.8303-2.7865A4.504 4.504 0 012.3408 7.872zm16.5963 3.8558L13.1038 8.364 15.1192 7.2a.0757.0757 0 01.071 0l4.8303 2.7913a4.4944 4.4944 0 01-.6765 8.1042v-5.6772a.79.79 0 00-.407-.667zm2.0107-3.0231l-.142-.0852-4.7735-2.7818a.7759.7759 0 00-.7854 0L9.409 9.2297V6.8974a.0662.0662 0 01.0284-.0615l4.8303-2.7866a4.4992 4.4992 0 016.6802 4.66zM8.3065 12.863l-2.02-1.1638a.0804.0804 0 01-.038-.0567V6.0742a4.4992 4.4992 0 017.3757-3.4537l-.142.0805L8.704 5.459a.7948.7948 0 00-.3927.6813zm1.0976-2.3654l2.602-1.4998 2.6069 1.4998v2.9994l-2.5974 1.4997-2.6067-1.4997z" />
    </svg>
  );
}

function OpenCodeIcon({ size }: { size: number }) {
  return (
    <svg
      aria-hidden="true"
      className="agent-brand-icon opencode-agent-icon"
      height={size}
      role="img"
      viewBox="0 6 24 30"
      width={size}
    >
      <path d="M18 30H6V18H18V30Z" fill="currentColor" opacity="0.42" />
      <path d="M18 12H6V30H18V12ZM24 36H0V6H24V36Z" fill="currentColor" />
    </svg>
  );
}
