import type {
  AgentCollectionSnapshot,
  AgentSettingsDetail,
} from "@openaide/app-server-client";
import {
  agentCatalogEntry,
  normalizedAgentIcon,
  type AgentIconId,
  type AgentSettingsRecord,
  type CustomAgentCreateParams,
  type CustomAgentMetadataUpdateParams,
} from "@openaide/app-shell-contracts";
import type { AppState } from "../state/store";

export function agentSettingsRecordFromProtocol(agent: AgentSettingsDetail): AgentSettingsRecord {
  return {
    id: agent.agentId,
    label: agent.label,
    enabled: agent.enabled,
    scope: "global",
    source_kind: agent.sourceKind === "builtIn" ? "built_in" : "custom",
    icon: normalizedAgentIcon(agent.icon) ?? "bot",
    transport: "stdio",
    status: agentSettingsStatusFromProtocol(agent.status),
    launch_label: agent.launchLabel,
    command_line: agent.commandLine ?? undefined,
    env: agent.env?.map((row) => row.secret
      ? { name: row.name, secret: true }
      : { name: row.name, value: row.value ?? undefined, secret: false }),
    description: agent.description,
    capabilities: agent.capabilities ?? [],
    auth_methods: agent.authMethods?.map((method) => ({
      id: method.id,
      label: method.label,
      kind: method.kind,
      description: method.description ?? undefined,
      variables: method.variables?.map((variable) => ({
        name: variable.name,
        label: variable.label ?? undefined,
        secret: variable.secret,
        optional: variable.optional,
      })),
      link: method.link ?? undefined,
      terminal_args: method.terminalArgs,
      terminal_env: method.terminalEnv,
    })) ?? [],
    logout_supported: agent.logoutSupported,
    authenticating_method_id: agent.authenticatingMethodId ?? undefined,
  };
}

export function settingsRecordFromCustomPayload(
  agentId: string,
  payload: CustomAgentCreateParams,
): AgentSettingsRecord {
  return {
    id: agentId,
    label: payload.label,
    enabled: payload.enabled,
    scope: "global",
    source_kind: "custom",
    icon: payload.icon,
    transport: "stdio",
    status: payload.enabled ? "disconnected" : "disabled",
    launch_label: payload.command_line,
    command_line: payload.command_line,
    env: payload.env.map((row) => row.secret
      ? { name: row.name, secret: true }
      : { ...row }),
    description: "Custom ACP stdio Agent",
    capabilities: [],
    auth_methods: [],
    logout_supported: false,
    authenticating_method_id: undefined,
  };
}

export function settingsRecordWithMetadata(
  state: AppState,
  agentId: string,
  payload: CustomAgentMetadataUpdateParams,
  agents: AgentCollectionSnapshot,
): AgentSettingsRecord {
  return {
    ...settingsRecordWithEnabled(state, agentId, payload.enabled, agents),
    label: payload.label,
    icon: payload.icon,
  };
}

export function settingsRecordWithEnabled(
  state: AppState,
  agentId: string,
  enabled: boolean,
  agents: AgentCollectionSnapshot,
): AgentSettingsRecord {
  const existing = state.settings.agentDetails?.find((agent) => agent.id === agentId);
  if (existing) {
    return {
      ...existing,
      enabled,
      status: enabled ? (existing.status === "disabled" ? "disconnected" : existing.status) : "disabled",
    };
  }
  const protocolAgent = agents.agents.find((agent) => agent.agentId === agentId);
  const known = agentCatalogEntry(agentId);
  return {
    id: agentId,
    label: protocolAgent?.label ?? known?.label ?? agentId,
    enabled,
    scope: "global",
    source_kind: known?.source_kind ?? "built_in",
    icon: (known?.icon ?? "bot") as AgentIconId,
    transport: "stdio",
    status: enabled ? "disconnected" : "disabled",
    launch_label: known?.command_line ?? "App Server managed",
    command_line: known?.source_kind === "custom" ? known.command_line : undefined,
    description: known?.description ?? "Agent available from App Server.",
    capabilities: [],
    auth_methods: [],
    logout_supported: false,
    authenticating_method_id: undefined,
  };
}

function agentSettingsStatusFromProtocol(status: AgentSettingsDetail["status"]): AgentSettingsRecord["status"] {
  switch (status) {
    case "setupRequired":
      return "setup_required";
    case "authRequired":
      return "auth_required";
    case "authenticating":
      return "authenticating";
    default:
      return status;
  }
}
