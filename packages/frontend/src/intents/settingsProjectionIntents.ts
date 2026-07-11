import {
  SETTINGS_GET_AGENT_DETAILS,
  SETTINGS_GET_MCP_SERVERS,
  SETTINGS_GET_SKILLS,
  type BackendConnection,
} from "@openaide/app-server-client";
import type { AppAction } from "../state/appReducer";
import type { AgentOption } from "../state/composerOptions";
import type { AppState } from "../state/store";
import {
  mapMcpServersProjection,
  mapSkillsProjection,
} from "../state/settingsProjectionMapping";
import {
  agentSettingsRecordFromProtocol,
} from "./agentSettingsRecords";

type SettingsProjectionConnection = Pick<BackendConnection, "request">;

export type SettingsProjectionIntentContext = {
  backendConnection?: SettingsProjectionConnection;
  currentAgentId: string;
  dispatch: (action: AppAction) => void;
  setAgents: (agents: AgentOption[]) => void;
  state: AppState;
};

export async function refreshSettingsProjectionsThroughBackend(
  context: SettingsProjectionIntentContext,
) {
  const backendConnection = context.backendConnection;
  if (!backendConnection) return false;

  await Promise.all([
    refreshAgentDetails(context, backendConnection),
    refreshMcpServers(context, backendConnection),
    refreshSkills(context, backendConnection),
  ]);
  return true;
}

async function refreshAgentDetails(
  context: SettingsProjectionIntentContext,
  backendConnection: SettingsProjectionConnection,
) {
  const result = await backendConnection.request(SETTINGS_GET_AGENT_DETAILS, {});
  context.dispatch({
    type: "settings:agentDetailsResult",
    generatedAt: result.generatedAt,
    agents: result.agents.map(agentSettingsRecordFromProtocol),
  });
}

async function refreshMcpServers(
  context: SettingsProjectionIntentContext,
  backendConnection: SettingsProjectionConnection,
) {
  context.dispatch({ type: "settings:mcpServersStart" });
  try {
    const result = await backendConnection.request(SETTINGS_GET_MCP_SERVERS, {});
    context.dispatch({ type: "settings:mcpServersResult", ...mapMcpServersProjection(result) });
  } catch (error) {
    context.dispatch({ type: "settings:mcpServersError", message: safeErrorMessage(error) });
  }
}

async function refreshSkills(
  context: SettingsProjectionIntentContext,
  backendConnection: SettingsProjectionConnection,
) {
  context.dispatch({ type: "settings:skillsStart" });
  try {
    const result = await backendConnection.request(SETTINGS_GET_SKILLS, {});
    context.dispatch({ type: "settings:skillsResult", ...mapSkillsProjection(result) });
  } catch (error) {
    context.dispatch({ type: "settings:skillsError", message: safeErrorMessage(error) });
  }
}

function safeErrorMessage(error: unknown) {
  return error instanceof Error ? error.message : "Unable to load Settings projection";
}
