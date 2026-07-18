import type {
  AgentIconId,
  AgentSettingsRecord,
  CustomAgentEnvRecord,
} from "@openaide/app-shell-contracts";
import { sameAgentLaunchIdentity } from "../../state/agentLaunchIdentity";

export type AgentDraft = {
  agent_id?: string;
  label: string;
  icon: AgentIconId;
  command_line: string;
  enabled: boolean;
  env: CustomAgentEnvRecord[];
};

export type AgentAuthMethod = AgentSettingsRecord["auth_methods"][number];

export function draftFromAgent(agent: AgentSettingsRecord): AgentDraft {
  return {
    agent_id: agent.id,
    label: agent.label,
    icon: agent.icon,
    command_line: agent.command_line ?? "",
    enabled: agent.enabled,
    env: agent.env ?? [],
  };
}

export function newAgentDraft(): AgentDraft {
  return { label: "", icon: "bot", command_line: "", enabled: true, env: [] };
}

export function draftChangesLaunch(agent: AgentSettingsRecord, draft: AgentDraft) {
  return !sameAgentLaunchIdentity(agent, draft);
}

export function agentStatusCopy(agent: AgentSettingsRecord) {
  if (agent.status === "connected" || agent.status === "ready") return "Ready for new tasks.";
  if (agent.status === "auth_required") return "Authentication is required before this agent can start work.";
  if (agent.status === "authenticating") return "Authentication is in progress.";
  if (agent.status === "setup_required") return "Setup is incomplete.";
  if (agent.status === "unsupported") return "This process launched, but did not satisfy OpenAIDE's ACP requirements.";
  if (agent.status === "disabled") return "Disabled in settings.";
  if (agent.status === "failed") return "Connection check failed.";
  return "Status check needed. Refresh to verify this agent.";
}

export function shouldConsumeAgentSaveAck({
  hasDraft,
  pendingSaveAgentId,
  savedAgentId,
}: {
  hasDraft: boolean;
  pendingSaveAgentId?: string;
  savedAgentId?: string;
}) {
  if (!savedAgentId || !hasDraft || !pendingSaveAgentId) return false;
  return pendingSaveAgentId === "__new__" || pendingSaveAgentId === savedAgentId;
}

export function shouldConsumeAgentDeleteAck({
  deletedAgentId,
  pendingDeleteAgentId,
}: {
  deletedAgentId?: string;
  pendingDeleteAgentId?: string;
}) {
  return Boolean(deletedAgentId && pendingDeleteAgentId === deletedAgentId);
}
