import {
  SETTINGS_RESET_TASK_HISTORY,
  SETTINGS_UPDATE_PREFERENCES,
  SETTINGS_UPDATE_RUNTIME,
} from "@openaide/app-server-client";
import { currentFrontendShell } from "../services/frontendShell";
import { postHostMessage, replaceSettingsTabRoute } from "../services/hostBridge";
import { mapProtocolAppPreferences, protocolComposerSubmitShortcut } from "../state/appPreferencesMapping";
import { mapProtocolRuntimeSettings } from "../state/runtimeSettingsMapping";
import type { AppCallbacksDependencies, SettingsCallbacks } from "./appControllerCallbackTypes";
import {
  authenticateAgentThroughBackend,
  cancelAgentAuthenticationThroughBackend,
  createCustomAgentThroughBackend,
  deleteCustomAgentThroughBackend,
  logoutAgentThroughBackend,
  refreshAgentSettingsThroughBackend,
  replaceCustomAgentThroughBackend,
  setAgentEnabledThroughBackend,
  updateCustomAgentMetadataThroughBackend,
} from "../intents/agentSettingsIntents";
import {
  loadSkillDetailsThroughBackend,
  refreshSettingsProjectionsThroughBackend,
} from "../intents/settingsProjectionIntents";
import {
  deleteMcpServerThroughBackend,
  loadMcpServerDetailsThroughBackend,
  saveMcpServerThroughBackend,
  setMcpServerEnabledThroughBackend,
} from "../intents/mcpSettingsIntents";

type SettingsDependencies = Pick<
  AppCallbacksDependencies,
  "backendConnection" | "dispatch" | "setAgents" | "setPreferences" | "state"
>;

export function createSettingsCallbacks({
  backendConnection,
  dispatch,
  setAgents,
  setPreferences,
  state,
}: SettingsDependencies): SettingsCallbacks {
  const agentSettingsContext = () => ({
    backendConnection: backendConnection?.request ? { request: backendConnection.request } : undefined,
    currentAgentId: state.newTask.selection.agentId,
    dispatch,
    setAgents: setAgents ?? (() => undefined),
    state,
  });
  return {
    // Sign-in Flow state is App Server-owned and arrives through the agents subscription, so
    // these callbacks only send intents; they never stage optimistic status.
    authenticateAgent: async (agentId, methodId, values) => {
      const agent = state.settings.agentDetails?.find((candidate) => candidate.id === agentId);
      const running = agent?.sign_in && agent.sign_in.phase !== "failed" ? agent.sign_in : undefined;
      const continuesTerminal = running?.phase === "awaiting_terminal" && running.method_id === methodId;
      if (running && !continuesTerminal) {
        // App Server admits one flow per Agent; switching methods ends the running one first.
        try {
          await cancelAgentAuthenticationThroughBackend(agentSettingsContext(), agentId);
        } catch {
          dispatch({ type: "settings:error", message: "Could not cancel sign-in." });
          return false;
        }
      }
      try {
        const outcome = await authenticateAgentThroughBackend(agentSettingsContext(), agentId, methodId, values);
        if (!outcome) dispatch({ type: "settings:error", message: appServerRequiredMessage() });
        return outcome === "authenticated";
      } catch {
        // The failure (or nothing, after a cancel) is already on the Agent's Sign-in Flow. Refresh
        // details so the projection catches up even if the subscription update is still in flight.
        await refreshAgentSettingsThroughBackend(agentSettingsContext()).catch(() => undefined);
        return false;
      }
    },
    cancelAgentAuthentication: async (agentId) => {
      try {
        const handled = await cancelAgentAuthenticationThroughBackend(agentSettingsContext(), agentId);
        if (!handled) dispatch({ type: "settings:error", message: appServerRequiredMessage() });
      } catch {
        dispatch({ type: "settings:error", message: "Could not cancel sign-in." });
      }
    },
    logoutAgent: async (agentId) => {
      try {
        const handled = await logoutAgentThroughBackend(agentSettingsContext(), agentId);
        if (!handled) dispatch({ type: "settings:error", message: appServerRequiredMessage() });
        return handled;
      } catch (error) {
        dispatch({ type: "settings:error", message: safeErrorMessage(error) });
        return false;
      }
    },
    createCustomAgent: (payload) => {
      dispatch({ type: "settings:start" });
      void createCustomAgentThroughBackend(agentSettingsContext(), payload)
        .then((handled) => {
          if (!handled) dispatch({ type: "settings:error", message: appServerRequiredMessage() });
        })
        .catch((error) => dispatch({ type: "settings:error", message: safeErrorMessage(error) }));
    },
    deleteCustomAgent: (agentId) => {
      dispatch({ type: "settings:start" });
      void deleteCustomAgentThroughBackend(agentSettingsContext(), agentId)
        .then((handled) => {
          if (!handled) dispatch({ type: "settings:error", message: appServerRequiredMessage() });
        })
        .catch((error) => dispatch({ type: "settings:error", message: safeErrorMessage(error) }));
    },
    deleteMcpServer: (server) => {
      dispatch({ type: "settings:start" });
      void deleteMcpServerThroughBackend(agentSettingsContext(), server)
        .catch((error) => dispatch({ type: "settings:error", message: safeErrorMessage(error) }));
    },
    getMcpServerDetails: (id) => loadMcpServerDetailsThroughBackend(agentSettingsContext(), id),
    getSkillDetails: (id) => loadSkillDetailsThroughBackend(agentSettingsContext(), id),
    refreshSettings: () => {
      dispatch({ type: "settings:start" });
      void refreshSettingsProjectionsThroughBackend(agentSettingsContext())
        .then((handled) => {
          if (!handled) dispatch({ type: "settings:error", message: settingsReadRequiredMessage() });
        })
        .catch((error) => dispatch({ type: "settings:error", message: safeErrorMessage(error) }));
    },
    resetTaskHistory: async () => {
      if (!backendConnection?.request) throw new Error(appServerRequiredMessage());
      await backendConnection.request(SETTINGS_RESET_TASK_HISTORY, {});
    },
    replaceCustomAgent: (payload) => {
      dispatch({ type: "settings:start" });
      void replaceCustomAgentThroughBackend(agentSettingsContext(), payload)
        .then((handled) => {
          if (!handled) dispatch({ type: "settings:error", message: appServerRequiredMessage() });
        })
        .catch((error) => dispatch({ type: "settings:error", message: safeErrorMessage(error) }));
    },
    saveMcpServer: (input) => {
      dispatch({ type: "settings:start" });
      void saveMcpServerThroughBackend(agentSettingsContext(), input)
        .catch((error) => dispatch({ type: "settings:error", message: safeErrorMessage(error) }));
    },
    selectSettingsTab: (tab) => {
      dispatch({ type: "settings:tab", tab });
      replaceSettingsTabRoute(tab);
    },
    setAcpTrace: (enabled) => {
      dispatch({ type: "settings:developerAcpTrace", enabled });
      if (!backendConnection?.request) {
        dispatch({ type: "settings:error", message: settingsReadRequiredMessage() });
        return;
      }
      void backendConnection.request(SETTINGS_UPDATE_RUNTIME, {
        developer: { acpTrace: { enabled } },
      })
        .then((settings) => dispatch({ type: "settings:runtimeSettings", settings: mapProtocolRuntimeSettings(settings) }))
        .catch((error) => dispatch({ type: "settings:error", message: safeErrorMessage(error) }));
    },
    setAgentEnabled: (agentId, enabled) => {
      dispatch({ type: "settings:start" });
      void setAgentEnabledThroughBackend(agentSettingsContext(), agentId, enabled)
        .then((handled) => {
          if (!handled) dispatch({ type: "settings:error", message: appServerRequiredMessage() });
        })
        .catch((error) => dispatch({ type: "settings:error", message: safeErrorMessage(error) }));
    },
    setMcpServerEnabled: (id, enabled) => {
      void setMcpServerEnabledThroughBackend(agentSettingsContext(), id, enabled)
        .catch((error) => dispatch({ type: "settings:error", message: safeErrorMessage(error) }));
    },
    setComposerSubmitShortcut: (shortcut) => {
      const nextPreferences = { composer_submit_shortcut: shortcut };
      setPreferences(nextPreferences);
      dispatch({ type: "settings:preferences", preferences: nextPreferences });
      if (!backendConnection?.request) {
        dispatch({ type: "settings:error", message: appServerRequiredMessage() });
        return;
      }
      void backendConnection.request(SETTINGS_UPDATE_PREFERENCES, {
        preferences: { composerSubmitShortcut: protocolComposerSubmitShortcut(shortcut) },
      })
        .then((result) => {
          const preferences = mapProtocolAppPreferences(result);
          setPreferences(preferences);
          dispatch({ type: "settings:preferences", preferences });
        })
        .catch((error) => dispatch({ type: "settings:error", message: safeErrorMessage(error) }));
    },
    unlockDeveloperSettings: () => {
      dispatch({ type: "settings:start" });
      postHostMessage({ type: "developer.settings.unlock" });
    },
    updateCustomAgentMetadata: (payload) => {
      dispatch({ type: "settings:start" });
      void updateCustomAgentMetadataThroughBackend(agentSettingsContext(), payload)
        .then((handled) => {
          if (!handled) dispatch({ type: "settings:error", message: appServerRequiredMessage() });
        })
        .catch((error) => dispatch({ type: "settings:error", message: safeErrorMessage(error) }));
    },
  };
}

function safeErrorMessage(error: unknown) {
  return error instanceof Error ? error.message : "Agent settings request failed";
}

function appServerRequiredMessage() {
  return "Agent catalog changes require the App Server.";
}

function settingsReadRequiredMessage() {
  return "Settings require the App Server.";
}
