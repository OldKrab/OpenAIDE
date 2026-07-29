import {
  MCP_CREATE_SERVER,
  MCP_DELETE_SERVER,
  MCP_GET_SERVER_DETAILS,
  MCP_SET_SERVER_ENABLED,
  MCP_UPDATE_SERVER,
  type BackendConnection,
  type McpServerDefinition,
} from "@openaide/app-server-client";
import type { McpSecretRef, SecretSyncPayload } from "@openaide/app-shell-contracts";
import {
  beginAgentSecretTransaction,
  type AgentSecretTransaction,
} from "../services/agentSecretTransaction";
import type { AppAction } from "../state/appReducer";
import { mapMcpServersProjection } from "../state/settingsProjectionMapping";

type McpSettingsConnection = Pick<BackendConnection, "request">;

export type McpSecretValue = {
  field: "env" | "header";
  name: string;
  value: string;
};

export type McpServerSaveInput = {
  server: McpServerDefinition;
  previous?: McpServerDefinition;
  secretValues: McpSecretValue[];
};

export type McpSettingsIntentContext = {
  backendConnection?: McpSettingsConnection;
  dispatch: (action: AppAction) => void;
};

export async function loadMcpServerDetailsThroughBackend(
  context: McpSettingsIntentContext,
  id: string,
) {
  const connection = requireConnection(context);
  return (await connection.request(MCP_GET_SERVER_DETAILS, { id })).server;
}

export async function saveMcpServerThroughBackend(
  context: McpSettingsIntentContext,
  input: McpServerSaveInput,
) {
  const connection = requireConnection(context);
  const changes = mcpSecretChanges(input);
  const transaction = changes.writes.length || changes.deletes.length
    ? await beginAgentSecretTransaction(changes)
    : undefined;
  const result = await requestWithSecretRollback(transaction, () => input.previous
    ? connection.request(MCP_UPDATE_SERVER, {
        server: input.server,
        expectedSecretNames: secretRefs(input.previous).map((reference) => reference.name),
      })
    : connection.request(MCP_CREATE_SERVER, { server: input.server }));
  await transaction?.commit();
  applyMcpMutationResult(context, result.servers);
  return result.serverId;
}

export async function deleteMcpServerThroughBackend(
  context: McpSettingsIntentContext,
  server: McpServerDefinition,
) {
  const connection = requireConnection(context);
  const references = secretRefs(server);
  const transaction = references.length
    ? await beginAgentSecretTransaction({ writes: [], deletes: references })
    : undefined;
  const result = await requestWithSecretRollback(transaction, () => connection.request(
    MCP_DELETE_SERVER,
    { id: server.id, expectedSecretNames: references.map((reference) => reference.name) },
  ));
  await transaction?.commit();
  applyMcpMutationResult(context, result.servers);
}

export async function setMcpServerEnabledThroughBackend(
  context: McpSettingsIntentContext,
  id: string,
  enabled: boolean,
) {
  const connection = requireConnection(context);
  const result = await connection.request(MCP_SET_SERVER_ENABLED, { id, enabled });
  applyMcpMutationResult(context, result.servers);
}

function mcpSecretChanges(input: McpServerSaveInput): SecretSyncPayload {
  const next = secretRefs(input.server);
  const previous = input.previous ? secretRefs(input.previous) : [];
  const nextKeys = new Set(next.map(secretRefKey));
  const values = new Map(
    input.secretValues.map((value) => [`${value.field}:${value.name}`, value.value]),
  );
  return {
    writes: next.flatMap((target) => {
      const value = values.get(`${target.field}:${target.name}`);
      return value ? [{ target, value }] : [];
    }),
    deletes: previous.filter((reference) => !nextKeys.has(secretRefKey(reference))),
  };
}

function secretRefs(server: McpServerDefinition): McpSecretRef[] {
  const configuration = server.configuration;
  const field = configuration.transport === "stdio" ? "env" as const : "header" as const;
  const names = configuration.transport === "stdio"
    ? configuration.secretEnv ?? []
    : configuration.secretHeaders ?? [];
  return [...new Set(names)].map((name) => ({
    kind: "mcp",
    serverId: server.id,
    field,
    name,
  }));
}

function secretRefKey(reference: McpSecretRef) {
  return `${reference.serverId}:${reference.field}:${reference.name}`;
}

function requireConnection(context: McpSettingsIntentContext) {
  if (!context.backendConnection) throw new Error("MCP settings require the App Server.");
  return context.backendConnection;
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
        throw new Error("MCP settings failed and secure storage could not be restored.");
      }
    }
    throw error;
  }
}

function applyMcpMutationResult(
  context: McpSettingsIntentContext,
  result: Parameters<typeof mapMcpServersProjection>[0],
) {
  context.dispatch({
    type: "settings:mcpServersResult",
    ...mapMcpServersProjection(result),
  });
}
