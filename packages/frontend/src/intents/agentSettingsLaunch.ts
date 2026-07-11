import type { CustomAgentCreateParams } from "@openaide/app-shell-contracts";
import { sameAgentLaunchIdentity } from "../state/agentLaunchIdentity";
import type { AppState } from "../state/store";

export function customAgentEnvObject(payload: CustomAgentCreateParams) {
  return Object.fromEntries(payload.env.filter((row) => !row.secret).map((row) => [row.name, row.value ?? ""]));
}

export function customAgentSecretEnv(payload: CustomAgentCreateParams) {
  return payload.env.filter((row) => row.secret).map((row) => row.name);
}

export function customAgentLaunchChanged(state: AppState, agentId: string, payload: CustomAgentCreateParams) {
  const existing = agentFromSettingsState(state, agentId);
  if (!existing) return true;
  return !sameAgentLaunchIdentity(existing, payload);
}

function agentFromSettingsState(state: AppState, agentId: string) {
  return state.settings.agentDetails?.find((agent) => agent.id === agentId);
}
