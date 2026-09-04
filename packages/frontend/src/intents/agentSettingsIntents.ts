import {
  AGENT_AUTHENTICATE,
  AGENT_CANCEL_AUTHENTICATE,
  AGENT_CREATE_CUSTOM,
  AGENT_DELETE_CUSTOM,
  AGENT_LOGOUT,
  AGENT_PROBE,
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
import { agentOptionsFromProtocol, fallbackAgentActionFromProtocol } from "../state/appServerAgents";
import { agentSettingsStatusFromProtocol, agentSignInFlowFromProtocol } from "./agentSettingsRecords";
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
  // The catalog mutation is durable before the potentially slow process check.
  // Release the editor now so a broken command remains editable and deletable.
  context.dispatch({
    type: "settings:agentSaved",
    agentId: result.agentId,
    agent: settingsRecordFromCustomPayload(result.agentId, payload),
  });
  await refreshSavedAgentSettings(context, result.agentId, payload.enabled);
  return true;
}

export async function authenticateAgentThroughBackend(
  context: AgentSettingsIntentContext,
  agentId: string,
  methodId: string,
  values?: Record<string, string>,
) {
  const backendConnection = context.backendConnection;
  if (!backendConnection) return false;
  const agent = context.state.settings.agentDetails?.find((candidate) => candidate.id === agentId);
  const method = agent?.auth_methods.find((candidate) => candidate.id === methodId);
  if (!agent || !method) throw new Error("Refresh Agent settings before authenticating.");
  const storageAgentId = authSecretStorageAgentId(agentId, methodId);
  const variables = method.variables ?? [];
  const secretVariables = variables.filter((variable) => variable.secret);
  const plainVariables = variables.filter((variable) => !variable.secret);
  const secretWrites = secretVariables.flatMap((variable) => {
    const value = values?.[variable.name];
    return value
      ? [{ target: { kind: "agentEnvironment" as const, agentId: storageAgentId, name: variable.name }, value }]
      : [];
  });
  const secretTransaction = secretWrites.length
    ? await beginAgentSecretTransaction({ writes: secretWrites, deletes: [] })
    : undefined;
  const result = await requestWithSecretRollback(secretTransaction, () => backendConnection.request(AGENT_AUTHENTICATE, {
    agentId: agentId as AgentId,
    methodId,
    ...(method.kind === "env_var" ? {
      env: Object.fromEntries(plainVariables.flatMap((variable) => {
        const value = values?.[variable.name];
        return value ? [[variable.name, value]] : [];
      })),
      secretEnv: secretVariables.map((variable) => variable.name),
      secretStorageAgentId: storageAgentId,
    } : {}),
    ...(method.kind === "terminal" ? {
      terminalConfirmed: agent.sign_in?.phase === "awaiting_terminal" && agent.sign_in.method_id === methodId,
    } : {}),
  }));
  await secretTransaction?.commit();
  await refreshAgentSettingsThroughBackend(context);
  return result.status === "authenticated" ? "authenticated" as const : "awaitingUser" as const;
}

export async function cancelAgentAuthenticationThroughBackend(
  context: AgentSettingsIntentContext,
  agentId: string,
) {
  const backendConnection = context.backendConnection;
  if (!backendConnection) return false;
  const result = await backendConnection.request(AGENT_CANCEL_AUTHENTICATE, { agentId: agentId as AgentId });
  applyAgentMutationResult(context, result.agents);
  await refreshAgentSettingsThroughBackend(context);
  return true;
}

export async function logoutAgentThroughBackend(
  context: AgentSettingsIntentContext,
  agentId: string,
) {
  const backendConnection = context.backendConnection;
  if (!backendConnection) return false;
  const agent = context.state.settings.agentDetails?.find((candidate) => candidate.id === agentId);
  if (!agent) throw new Error("Refresh Agent settings before signing out.");
  const methodId = agent.last_authentication_method_id;
  const method = agent.auth_methods.find((candidate) => candidate.id === methodId);
  const secretDeletes = method?.kind === "env_var"
    ? (method.variables ?? []).filter((variable) => variable.secret).map((variable) => ({
      kind: "agentEnvironment" as const,
      agentId: authSecretStorageAgentId(agentId, method.id),
      name: variable.name,
    }))
    : [];
  const secretTransaction = secretDeletes.length
    ? await beginAgentSecretTransaction({ writes: [], deletes: secretDeletes })
    : undefined;
  const result = await requestWithSecretRollback(secretTransaction, () => backendConnection.request(AGENT_LOGOUT, {
    agentId: agentId as AgentId,
    expectedMethodId: methodId,
  }));
  await secretTransaction?.commit();
  applyAgentMutationResult(context, result.agents);
  await refreshAgentSettingsThroughBackend(context);
  return true;
}

function authSecretStorageAgentId(agentId: string, methodId: string) {
  const encodedMethod = [...methodId]
    .map((character) => character.codePointAt(0)!.toString(16))
    .join("-");
  return `${agentId}.auth.${encodedMethod}`;
}

export async function updateCustomAgentMetadataThroughBackend(
  context: AgentSettingsIntentContext,
  payload: CustomAgentMetadataUpdateParams,
) {
  const backendConnection = context.backendConnection;
  if (!backendConnection) return false;
  const wasEnabled = context.state.settings.agentDetails
    ?.find((agent) => agent.id === payload.agent_id)
    ?.enabled;
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
  if (payload.enabled && wasEnabled === false) {
    await probeAgentSettingsThroughBackend(context, result.agentId);
  }
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
  await refreshSavedAgentSettings(context, result.newAgentId, payload.enabled);
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
  if (enabled) {
    await probeAgentSettingsThroughBackend(context, agentId);
  }
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

async function probeAgentSettingsThroughBackend(
  context: AgentSettingsIntentContext,
  agentId: string,
) {
  const backendConnection = context.backendConnection;
  if (!backendConnection) return false;
  const result = await backendConnection.request(AGENT_PROBE, { agentId: agentId as AgentId });
  applyAgentMutationResult(context, result.agents);
  return refreshAgentSettingsThroughBackend(context);
}

function refreshSavedAgentSettings(
  context: AgentSettingsIntentContext,
  agentId: string,
  enabled: boolean,
) {
  return enabled
    ? probeAgentSettingsThroughBackend(context, agentId)
    : refreshAgentSettingsThroughBackend(context);
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
  context.dispatch({
    type: "settings:agentCollection",
    agents: agents.agents.map((agent) => ({
      agentId: agent.agentId,
      status: agentSettingsStatusFromProtocol(agent.status),
      setupReason: agent.setupReason ?? undefined,
      signIn: agentSignInFlowFromProtocol(agent.signIn),
    })),
  });
  const action = fallbackAgentActionFromProtocol(agents, context.currentAgentId);
  if (action) context.dispatch(action);
}
