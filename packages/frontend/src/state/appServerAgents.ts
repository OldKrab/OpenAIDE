import type { AgentCollectionSnapshot } from "@openaide/app-server-client";
import { agentCatalogEntry } from "@openaide/app-shell-contracts";
import type { AppAction } from "./appReducer";
import type { AgentOption } from "./composerOptions";

export function applyProtocolAgents(
  snapshot: AgentCollectionSnapshot | null | undefined,
  currentAgentId: string,
  setAgents: (agents: AgentOption[]) => void,
  dispatch: (action: AppAction) => void,
) {
  if (!snapshot) return;
  setAgents(agentOptionsFromProtocol(snapshot));
  const action = fallbackAgentActionFromProtocol(snapshot, currentAgentId);
  if (action) dispatch(action);
}

export function agentOptionsFromProtocol(snapshot: AgentCollectionSnapshot): AgentOption[] {
  return snapshot.agents.map((agent) => {
    const known = agentCatalogEntry(agent.agentId);
    return {
      id: agent.agentId,
      label: agent.label,
      description: known?.description ?? "Agent available from App Server.",
      icon: known?.icon ?? "bot",
      enabled: true,
      status: agent.status,
      setupReason: agent.setupReason ?? undefined,
    };
  });
}

export function fallbackAgentActionFromProtocol(
  snapshot: AgentCollectionSnapshot,
  currentAgentId = "",
): AppAction | undefined {
  if (snapshot.agents.some((agent) => agent.agentId === currentAgentId)) return undefined;
  const fallbackAgent = snapshot.agents[0];
  if (!fallbackAgent) return undefined;
  return {
    type: "newTask:agent",
    agentId: fallbackAgent.agentId,
    agentLabel: fallbackAgent.label,
  };
}
