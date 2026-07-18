import { SETTINGS_UPDATE_PREFERENCES, SETTINGS_UPDATE_RUNTIME } from "@openaide/app-server-client";
import { postHostMessage, replaceSettingsTabRoute } from "../services/hostBridge";
import { mapProtocolAppPreferences, protocolComposerSubmitShortcut } from "../state/appPreferencesMapping";
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
import { refreshSettingsProjectionsThroughBackend } from "../intents/settingsProjectionIntents";

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
    authenticateAgent: (agentId, methodId, values) => {
      dispatch({ type: "settings:start" });
      void authenticateAgentThroughBackend(agentSettingsContext(), agentId, methodId, values)
        .then((handled) => {
          if (!handled) dispatch({ type: "settings:error", message: appServerRequiredMessage() });
        })
        .catch(() => dispatch({ type: "settings:error", message: authenticationFailedMessage() }));
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
    refreshSettings: () => {
      dispatch({ type: "settings:start" });
      void refreshSettingsProjectionsThroughBackend(agentSettingsContext())
        .then((handled) => {
          if (!handled) dispatch({ type: "settings:error", message: settingsReadRequiredMessage() });
        })
        .catch((error) => dispatch({ type: "settings:error", message: safeErrorMessage(error) }));
    },
    replaceCustomAgent: (payload) => {
      dispatch({ type: "settings:start" });
      void replaceCustomAgentThroughBackend(agentSettingsContext(), payload)
        .then((handled) => {
          if (!handled) dispatch({ type: "settings:error", message: appServerRequiredMessage() });
        })
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

function authenticationFailedMessage() {
  return "Authentication failed. Check the Agent's requirements and try again.";
}

function settingsReadRequiredMessage() {
  return "Settings require the App Server.";
}
