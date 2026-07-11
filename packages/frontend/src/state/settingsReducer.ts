import type {
  AgentSettingsRecord,
  AppPreferencesRecord,
  McpServerSettingsRecord,
  RuntimeSettingsResult,
  SettingsProjectionAvailability,
  SettingsTabId,
  SkillSettingsRecord,
} from "@openaide/app-shell-contracts";
import type { AppAction } from "./appReducer";
import type { AppState } from "./store";

type SettingsAction =
  | { type: "settings:start" }
  | { type: "settings:sections"; tabs: SettingsTabId[] }
  | { type: "settings:agentDetailsResult"; generatedAt: string; agents: AgentSettingsRecord[] }
  | { type: "settings:mcpServersStart" }
  | { type: "settings:mcpServersResult"; generatedAt: string; availability: SettingsProjectionAvailability; servers: McpServerSettingsRecord[] }
  | { type: "settings:mcpServersError"; message: string }
  | { type: "settings:skillsStart" }
  | { type: "settings:skillsResult"; generatedAt: string; availability: SettingsProjectionAvailability; skills: SkillSettingsRecord[] }
  | { type: "settings:skillsError"; message: string }
  | { type: "settings:error"; message: string }
  | { type: "settings:agentSaved"; agentId: string; agent?: AgentSettingsRecord }
  | { type: "settings:agentReplaced"; oldAgentId: string; newAgentId: string; agent?: AgentSettingsRecord }
  | { type: "settings:agentUpdated"; agent: AgentSettingsRecord }
  | { type: "settings:agentDeleted"; agentId: string }
  | { type: "settings:preferences"; preferences: AppPreferencesRecord }
  | { type: "settings:developerAcpTrace"; enabled: boolean }
  | { type: "settings:runtimeSettings"; settings: RuntimeSettingsResult }
  | { type: "settings:tab"; tab: SettingsTabId };

export function reduceSettingsState(state: AppState, action: AppAction): AppState | undefined {
  if (!isSettingsAction(action)) return undefined;
  switch (action.type) {
    case "settings:start":
      return {
        ...state,
        settings: { ...state.settings, loading: true, error: undefined, savedAgentId: undefined, deletedAgentId: undefined },
      };
    case "settings:sections":
      return {
        ...state,
        settings: {
          ...state.settings,
          availableTabs: action.tabs,
          activeTab: action.tabs.includes(state.settings.activeTab) ? state.settings.activeTab : action.tabs[0] ?? "agents",
        },
      };
    case "settings:agentDetailsResult":
      return {
        ...state,
        settings: {
          ...state.settings,
          loading: false,
          agentDetails: action.agents,
          agentDetailsGeneratedAt: action.generatedAt,
          error: undefined,
        },
      };
    case "settings:mcpServersStart":
      return {
        ...state,
        settings: { ...state.settings, mcpServersLoading: true, mcpServersError: undefined },
      };
    case "settings:mcpServersResult":
      return {
        ...state,
        settings: {
          ...state.settings,
          mcpServersAvailability: action.availability,
          mcpServers: action.servers,
          mcpServersGeneratedAt: action.generatedAt,
          mcpServersLoading: false,
          mcpServersError: undefined,
        },
      };
    case "settings:mcpServersError":
      return {
        ...state,
        settings: { ...state.settings, mcpServersLoading: false, mcpServersError: action.message },
      };
    case "settings:skillsStart":
      return {
        ...state,
        settings: { ...state.settings, skillsLoading: true, skillsError: undefined },
      };
    case "settings:skillsResult":
      return {
        ...state,
        settings: {
          ...state.settings,
          skillsAvailability: action.availability,
          skills: action.skills,
          skillsGeneratedAt: action.generatedAt,
          skillsLoading: false,
          skillsError: undefined,
        },
      };
    case "settings:skillsError":
      return {
        ...state,
        settings: { ...state.settings, skillsLoading: false, skillsError: action.message },
      };
    case "settings:preferences":
      return {
        ...state,
        settings: {
          ...state.settings,
          error: undefined,
        },
      };
    case "settings:developerAcpTrace":
      return {
        ...state,
        settings: updateRuntimeSettings(
          state.settings,
          (runtime) => ({
            developer: {
              ...runtime.developer,
              acp_trace: { ...runtime.developer.acp_trace, enabled: action.enabled },
            },
          }),
        ),
      };
    case "settings:runtimeSettings":
      return {
        ...state,
        settings: {
          ...state.settings,
          error: undefined,
          runtimeSettings: action.settings,
        },
      };
    case "settings:error":
      return {
        ...state,
        settings: { ...state.settings, loading: false, error: action.message },
      };
    case "settings:agentSaved":
      return {
        ...state,
        settings: {
          ...state.settings,
          loading: false,
          savedAgentId: action.agentId,
          deletedAgentId: undefined,
          agentDetails: action.agent && state.settings.agentDetails
            ? upsertAgent(state.settings.agentDetails, action.agent)
            : state.settings.agentDetails,
        },
      };
    case "settings:agentReplaced":
      return {
        ...state,
        settings: {
          ...state.settings,
          loading: false,
          savedAgentId: action.newAgentId,
          deletedAgentId: action.oldAgentId,
          agentDetails: action.agent && state.settings.agentDetails
            ? upsertAgent(
                state.settings.agentDetails.filter((agent) => agent.id !== action.oldAgentId),
                action.agent,
              )
            : state.settings.agentDetails?.filter((agent) => agent.id !== action.oldAgentId),
        },
      };
    case "settings:agentUpdated":
      return {
        ...state,
        settings: {
          ...state.settings,
          loading: false,
          error: undefined,
          agentDetails: state.settings.agentDetails
            ? upsertAgent(state.settings.agentDetails, action.agent)
            : state.settings.agentDetails,
        },
      };
    case "settings:agentDeleted":
      return {
        ...state,
        settings: {
          ...state.settings,
          loading: false,
          deletedAgentId: action.agentId,
          savedAgentId: undefined,
          agentDetails: state.settings.agentDetails
            ? state.settings.agentDetails.filter((agent) => agent.id !== action.agentId)
            : state.settings.agentDetails,
        },
      };
    case "settings:tab":
      return {
        ...state,
        settings: { ...state.settings, activeTab: action.tab },
      };
  }
}

function isSettingsAction(action: AppAction): action is SettingsAction {
  return action.type.startsWith("settings:");
}

function upsertAgent(agents: AgentSettingsRecord[], agent: AgentSettingsRecord): AgentSettingsRecord[] {
  return agents.some((item) => item.id === agent.id)
    ? agents.map((item) => (item.id === agent.id ? agent : item))
    : [...agents, agent];
}

function updateRuntimeSettings(
  settings: AppState["settings"],
  update: (runtime: RuntimeSettingsResult) => RuntimeSettingsResult,
): AppState["settings"] {
  return settings.runtimeSettings ? { ...settings, runtimeSettings: update(settings.runtimeSettings) } : settings;
}
