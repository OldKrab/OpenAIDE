import {
  AGENT_AUTHENTICATE,
  AGENT_CREATE_CUSTOM,
  AGENT_DELETE_CUSTOM,
  AGENT_REPLACE_CUSTOM,
  AGENT_SET_ENABLED,
  AGENT_UPDATE_CUSTOM_METADATA,
  SETTINGS_GET_AGENT_DETAILS,
  type AgentCollectionSnapshot,
  type AgentId,
  type BackendConnection,
} from "@openaide/app-server-client";
import {
  type CustomAgentCreateParams,
  type CustomAgentMetadataUpdateParams,
  type CustomAgentReplaceParams,
} from "@openaide/app-shell-contracts";
import { agentOptionsFromProtocol, defaultAgentActionFromProtocol } from "../state/appServerAgents";
import {
  beginAgentSecretTransaction,
  type AgentSecretTransaction,
} from "../services/agentSecretTransaction";
import type { AppAction } from "../state/appReducer";
import type { AgentOption } from "../state/composerOptions";
import type { AppState } from "../state/store";
import {
  customAgentEnvObject,
  customAgentSecretEnv,
} from "./agentSettingsLaunch";
import {
  agentSettingsRecordFromProtocol,
  settingsRecordFromCustomPayload,
  settingsRecordWithEnabled,
  settingsRecordWithMetadata,
} from "./agentSettingsRecords";
import {
  secretsForCreatedAgent,
  secretsForDeletedAgent,
  secretsForReplacedAgent,
} from "./agentSettingsSecrets";

type AgentSettingsConnection = Pick<BackendConnection, "request">;

export type AgentSettingsIntentContext = {
  backendConnection?: AgentSettingsConnection;
  currentAgentId: string;
  dispatch: (action: AppAction) => void;
  setAgents: (agents: AgentOption[]) => void;
  state: AppState;
};

export async function createCustomAgentThroughBackend(
  context: AgentSettingsIntentContext,
  payload: CustomAgentCreateParams,
) {
  const backendConnection = context.backendConnection;
  if (!backendConnection) return false;
  const command = parseAgentCommandLine(payload.command_line);
  const agentId = generatedCustomAgentId();
  const secretChanges = secretsForCreatedAgent(agentId, payload)
    ?? (payload.env.some((row) => row.secret) ? { writes: [], deletes: [] } : undefined);
  const secretTransaction = secretChanges
    ? await beginAgentSecretTransaction(secretChanges)
    : undefined;
  const result = await requestWithSecretRollback(secretTransaction, () => backendConnection.request(
    AGENT_CREATE_CUSTOM,
    {
      agentId: agentId as AgentId,
      label: payload.label,
      icon: payload.icon,
      commandLine: payload.command_line,
      command: command.command,
      args: command.args,
      env: customAgentEnvObject(payload),
      secretEnv: customAgentSecretEnv(payload),
      enabled: payload.enabled,
    },
  ));
  await secretTransaction?.commit();
  applyAgentMutationResult(context, result.agents);
  context.dispatch({
    type: "settings:agentSaved",
    agentId: result.agentId,
    agent: settingsRecordFromCustomPayload(result.agentId, payload),
  });
  return true;
}

export async function authenticateAgentThroughBackend(
  context: AgentSettingsIntentContext,
  agentId: string,
  methodId: string,
) {
  const backendConnection = context.backendConnection;
  if (!backendConnection) return false;
  await backendConnection.request(AGENT_AUTHENTICATE, {
    agentId: agentId as AgentId,
    methodId,
  });
  await refreshAgentSettingsThroughBackend(context);
  return true;
}

export async function updateCustomAgentMetadataThroughBackend(
  context: AgentSettingsIntentContext,
  payload: CustomAgentMetadataUpdateParams,
) {
  const backendConnection = context.backendConnection;
  if (!backendConnection) return false;
  const result = await backendConnection.request(AGENT_UPDATE_CUSTOM_METADATA, {
    agentId: payload.agent_id as AgentId,
    label: payload.label,
    icon: payload.icon,
    enabled: payload.enabled,
  });
  applyAgentMutationResult(context, result.agents);
  context.dispatch({
    type: "settings:agentSaved",
    agentId: result.agentId,
    agent: settingsRecordWithMetadata(context.state, result.agentId, payload, result.agents),
  });
  return true;
}

export async function replaceCustomAgentThroughBackend(
  context: AgentSettingsIntentContext,
  payload: CustomAgentReplaceParams,
) {
  const backendConnection = context.backendConnection;
  if (!backendConnection) return false;
  const command = parseAgentCommandLine(payload.command_line);
  const sourceSecretEnv = agentSecretNames(context.state, payload.source_agent_id);
  const targetAgentId = generatedCustomAgentId();
  const secretChanges = secretsForReplacedAgent(
    payload.source_agent_id,
    targetAgentId,
    sourceSecretEnv,
    payload,
  ) ?? (payload.env.some((row) => row.secret) ? { writes: [], deletes: [] } : undefined);
  const secretTransaction = secretChanges
    ? await beginAgentSecretTransaction(secretChanges)
    : undefined;
  const result = await requestWithSecretRollback(secretTransaction, () => backendConnection.request(
    AGENT_REPLACE_CUSTOM,
    {
      sourceAgentId: payload.source_agent_id as AgentId,
      targetAgentId: targetAgentId as AgentId,
      expectedSourceSecretEnv: sourceSecretEnv,
      label: payload.label,
      icon: payload.icon,
      commandLine: payload.command_line,
      command: command.command,
      args: command.args,
      env: customAgentEnvObject(payload),
      secretEnv: customAgentSecretEnv(payload),
      enabled: payload.enabled,
      confirmation: {
        acceptedLaunchIdentityChange: payload.confirmed,
      },
    },
  ));
  await secretTransaction?.commit();
  applyAgentMutationResult(context, result.agents);
  context.dispatch({
    type: "settings:agentReplaced",
    oldAgentId: result.oldAgentId,
    newAgentId: result.newAgentId,
    agent: settingsRecordFromCustomPayload(result.newAgentId, payload),
  });
  return true;
}

export async function deleteCustomAgentThroughBackend(
  context: AgentSettingsIntentContext,
  agentId: string,
) {
  const backendConnection = context.backendConnection;
  if (!backendConnection) return false;
  const secretEnv = agentSecretNames(context.state, agentId);
  const secretChanges = secretsForDeletedAgent(agentId, secretEnv);
  const secretTransaction = secretChanges
    ? await beginAgentSecretTransaction(secretChanges)
    : undefined;
  const result = await requestWithSecretRollback(secretTransaction, () => backendConnection.request(
    AGENT_DELETE_CUSTOM,
    { agentId: agentId as AgentId, expectedSecretEnv: secretEnv },
  ));
  await secretTransaction?.commit();
  applyAgentMutationResult(context, result.agents);
  context.dispatch({ type: "settings:agentDeleted", agentId: result.agentId });
  return true;
}

export async function setAgentEnabledThroughBackend(
  context: AgentSettingsIntentContext,
  agentId: string,
  enabled: boolean,
) {
  const backendConnection = context.backendConnection;
  if (!backendConnection) return false;
  const result = await backendConnection.request(AGENT_SET_ENABLED, { agentId: agentId as AgentId, enabled });
  applyAgentMutationResult(context, result.agents);
  context.dispatch({
    type: "settings:agentUpdated",
    agent: settingsRecordWithEnabled(context.state, agentId, enabled, result.agents),
  });
  return true;
}

function generatedCustomAgentId() {
  return `custom.${crypto.randomUUID()}`;
}

function agentSecretNames(state: AppState, agentId: string) {
  const agent = state.settings.agentDetails?.find((candidate) => candidate.id === agentId);
  if (!agent) throw new Error("Refresh Agent settings before changing its secure environment.");
  return [...new Set((agent.env ?? []).filter((row) => row.secret).map((row) => row.name))];
}

async function requestWithSecretRollback<T>(
  transaction: AgentSecretTransaction | undefined,
  request: () => Promise<T>,
) {
  try {
    return await request();
  } catch (error) {
    if (transaction) {
      try {
        await transaction.rollback();
      } catch {
        throw new Error("Agent settings failed and secure storage could not be restored.");
      }
    }
    throw error;
  }
}

export async function refreshAgentSettingsThroughBackend(context: AgentSettingsIntentContext) {
  const backendConnection = context.backendConnection;
  if (!backendConnection) return false;
  const result = await backendConnection.request(SETTINGS_GET_AGENT_DETAILS, {});
  context.dispatch({
    type: "settings:agentDetailsResult",
    generatedAt: result.generatedAt,
    agents: result.agents.map(agentSettingsRecordFromProtocol),
  });
  return true;
}

export function parseAgentCommandLine(input: string): { command: string; args: string[] } {
  const tokens: string[] = [];
  let current = "";
  let quote: "'" | '"' | undefined;
  let escaping = false;

  for (const char of input.trim()) {
    if (escaping) {
      current += char;
      escaping = false;
      continue;
    }
    if (char === "\\" && quote !== "'") {
      escaping = true;
      continue;
    }
    if ((char === "'" || char === '"') && !quote) {
      quote = char;
      continue;
    }
    if (char === quote) {
      quote = undefined;
      continue;
    }
    if (/\s/.test(char) && !quote) {
      if (current) {
        tokens.push(current);
        current = "";
      }
      continue;
    }
    current += char;
  }

  if (escaping) current += "\\";
  if (quote) throw new Error("Command has an unterminated quote.");
  if (current) tokens.push(current);
  const [command, ...args] = tokens;
  if (!command) throw new Error("Command is required.");
  return { command, args };
}

function applyAgentMutationResult(
  context: AgentSettingsIntentContext,
  agents: AgentCollectionSnapshot,
) {
  context.setAgents(agentOptionsFromProtocol(agents));
  const action = defaultAgentActionFromProtocol(agents, context.currentAgentId);
  if (action) context.dispatch(action);
}
