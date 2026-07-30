import {
  SETTINGS_RESET_TASK_HISTORY,
  SETTINGS_UPDATE_PREFERENCES,
  SETTINGS_UPDATE_RUNTIME,
} from "@openaide/app-server-client";
import type { AppPreferencesPatch } from "@openaide/app-server-client";
import type { AppPreferencesRecord } from "@openaide/app-shell-contracts";
import { postHostMessage, replaceSettingsTabRoute } from "../services/hostBridge";
import {
  mapProtocolAppPreferences,
  protocolAppTheme,
  protocolComposerSubmitShortcut,
} from "../state/appPreferencesMapping";
import { mapProtocolRuntimeSettings } from "../state/runtimeSettingsMapping";
import type { AppCallbacksDependencies, SettingsCallbacks } from "./appControllerCallbackTypes";
import {
  authenticateAgentThroughBackend,
  createCustomAgentThroughBackend,
  deleteCustomAgentThroughBackend,
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
  "backendConnection" | "dispatch" | "preferences" | "setAgents" | "setPreferences" | "state"
>;

export function createSettingsCallbacks({
  backendConnection,
  dispatch,
  preferences,
  setAgents,
  setPreferences,
  state,
}: SettingsDependencies): SettingsCallbacks {
  let optimisticPreferences: Required<AppPreferencesRecord> = {
    composer_submit_shortcut: preferences.composer_submit_shortcut,
    theme: preferences.theme ?? "system",
  };
  const updatePreferences = (
    nextPreferences: Required<AppPreferencesRecord>,
    patch: AppPreferencesPatch,
  ) => {
    optimisticPreferences = nextPreferences;
    setPreferences(nextPreferences);
    dispatch({ type: "settings:preferences", preferences: nextPreferences });
    if (!backendConnection?.request) {
      dispatch({ type: "settings:error", message: appServerRequiredMessage() });
      return;
    }
    void backendConnection.request(SETTINGS_UPDATE_PREFERENCES, { preferences: patch })
      .then((result) => {
        const confirmedPreferences = mapProtocolAppPreferences(result);
        optimisticPreferences = {
          composer_submit_shortcut: confirmedPreferences.composer_submit_shortcut,
          theme: confirmedPreferences.theme ?? "system",
        };
        setPreferences(confirmedPreferences);
        dispatch({ type: "settings:preferences", preferences: confirmedPreferences });
      })
      .catch((error) => dispatch({ type: "settings:error", message: safeErrorMessage(error) }));
  };
  const agentSettingsContext = () => ({
    backendConnection: backendConnection?.request ? { request: backendConnection.request } : undefined,
    currentAgentId: state.newTask.selection.agentId,
    dispatch,
    setAgents: setAgents ?? (() => undefined),
    state,
  });
  return {
    authenticateAgent: (agentId, methodId, values) => {
      dispatch({ type: "settings:start" });
      return authenticateAgentThroughBackend(agentSettingsContext(), agentId, methodId, values)
        .then((outcome) => {
          if (!outcome) dispatch({ type: "settings:error", message: appServerRequiredMessage() });
          return outcome === "authenticated";
        })
        .catch(() => {
          dispatch({ type: "settings:error", message: authenticationFailedMessage() });
          return false;
        });
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
      updatePreferences(
        { ...optimisticPreferences, composer_submit_shortcut: shortcut },
        { composerSubmitShortcut: protocolComposerSubmitShortcut(shortcut) },
      );
    },
    setTheme: (theme) => {
      updatePreferences(
        { ...optimisticPreferences, theme },
        { theme: protocolAppTheme(theme) },
      );
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

function authenticationFailedMessage() {
  return "Authentication failed. Check the Agent's requirements and try again.";
}

function settingsReadRequiredMessage() {
  return "Settings require the App Server.";
}
