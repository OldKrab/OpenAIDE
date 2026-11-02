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
import { createRemoteDeviceIntents } from "../intents/remoteDeviceIntents";
import { withSettingsRequestLog } from "../intents/settingsRequestLog";

type SettingsDependencies = Pick<
  AppCallbacksDependencies,
  "backendConnection" | "dispatch" | "setAgents" | "setPreferences" | "state"
>;

export function createSettingsCallbacks({
  backendConnection,
  dispatch: dispatchAction,
  setAgents,
  setPreferences,
  state,
}: SettingsDependencies): SettingsCallbacks {
  const dispatch: typeof dispatchAction = (action) => {
    // The message is free-form, so diagnostics carry only the section.
    if (action.type === "settings:error") console.warn(`settings_action_failed tab=${action.tab ?? "none"}`);
    dispatchAction(action);
  };
  const settingsConnection = () => backendConnection?.request
    ? withSettingsRequestLog({ request: backendConnection.request })
    : undefined;
  const agentSettingsContext = () => ({
    backendConnection: settingsConnection(),
    currentAgentId: state.newTask.selection.agentId,
    dispatch,
    setAgents: setAgents ?? (() => undefined),
    state,
  });
  const setAgentEnabled: SettingsCallbacks["setAgentEnabled"] = async (
    agentId,
    enabled,
    acceptedActiveWorkInterruption,
  ) => {
    dispatch({ type: "settings:start" });
    try {
      const outcome = await setAgentEnabledThroughBackend(
        agentSettingsContext(),
        agentId,
        enabled,
        acceptedActiveWorkInterruption,
      );
      if (outcome.kind === "unavailable") {
        dispatch({ type: "settings:error", tab: "agents", message: appServerRequiredMessage() });
      }
      return outcome;
    } catch (error) {
      dispatch({ type: "settings:error", tab: "agents", message: safeErrorMessage(error) });
      return { kind: "failed" };
    }
  };
  return {
    dismissError: () => dispatch({ type: "settings:error:clear" }),
    devices: createRemoteDeviceIntents(settingsConnection()),
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
          dispatch({ type: "settings:error", tab: "agents", message: "Could not cancel sign-in." });
          return false;
        }
      }
      try {
        const outcome = await authenticateAgentThroughBackend(agentSettingsContext(), agentId, methodId, values);
        if (!outcome) dispatch({ type: "settings:error", tab: "agents", message: appServerRequiredMessage() });
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
        if (!handled) dispatch({ type: "settings:error", tab: "agents", message: appServerRequiredMessage() });
      } catch {
        dispatch({ type: "settings:error", tab: "agents", message: "Could not cancel sign-in." });
      }
    },
    logoutAgent: async (agentId) => {
      try {
        const handled = await logoutAgentThroughBackend(agentSettingsContext(), agentId);
        if (!handled) dispatch({ type: "settings:error", tab: "agents", message: appServerRequiredMessage() });
        return handled;
      } catch (error) {
        dispatch({ type: "settings:error", tab: "agents", message: safeErrorMessage(error) });
        return false;
      }
    },
    createCustomAgent: (payload) => {
      dispatch({ type: "settings:start" });
      void createCustomAgentThroughBackend(agentSettingsContext(), payload)
        .then((handled) => {
          if (!handled) dispatch({ type: "settings:error", tab: "agents", message: appServerRequiredMessage() });
        })
        .catch((error) => dispatch({ type: "settings:error", tab: "agents", message: safeErrorMessage(error) }));
    },
    deleteCustomAgent: (agentId) => {
      dispatch({ type: "settings:start" });
      void deleteCustomAgentThroughBackend(agentSettingsContext(), agentId)
        .then((handled) => {
          if (!handled) dispatch({ type: "settings:error", tab: "agents", message: appServerRequiredMessage() });
        })
        .catch((error) => dispatch({ type: "settings:error", tab: "agents", message: safeErrorMessage(error) }));
    },
    deleteMcpServer: (server) => {
      dispatch({ type: "settings:start" });
      void deleteMcpServerThroughBackend(agentSettingsContext(), server)
        .catch((error) => dispatch({ type: "settings:error", tab: "mcp", message: safeErrorMessage(error) }));
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
      const connection = settingsConnection();
      if (!connection) throw new Error(appServerRequiredMessage());
      await connection.request(SETTINGS_RESET_TASK_HISTORY, {});
    },
    replaceCustomAgent: (payload) => {
      dispatch({ type: "settings:start" });
      void replaceCustomAgentThroughBackend(agentSettingsContext(), payload)
        .then((handled) => {
          if (!handled) dispatch({ type: "settings:error", tab: "agents", message: appServerRequiredMessage() });
        })
        .catch((error) => dispatch({ type: "settings:error", tab: "agents", message: safeErrorMessage(error) }));
    },
    saveMcpServer: (input) => {
      dispatch({ type: "settings:start" });
      void saveMcpServerThroughBackend(agentSettingsContext(), input)
        .catch((error) => dispatch({ type: "settings:error", tab: "mcp", message: safeErrorMessage(error) }));
    },
    selectSettingsTab: (tab) => {
      dispatch({ type: "settings:tab", tab });
      replaceSettingsTabRoute(tab);
    },
    setAcpTrace: (enabled) => {
      dispatch({ type: "settings:developerAcpTrace", enabled });
      const connection = settingsConnection();
      if (!connection) {
        dispatch({ type: "settings:error", tab: "troubleshooting", message: settingsReadRequiredMessage() });
        return;
      }
      void connection.request(SETTINGS_UPDATE_RUNTIME, {
        developer: { acpTrace: { enabled } },
      })
        .then((settings) => dispatch({ type: "settings:runtimeSettings", settings: mapProtocolRuntimeSettings(settings) }))
        .catch((error) => dispatch({ type: "settings:error", tab: "troubleshooting", message: safeErrorMessage(error) }));
    },
    setAgentEnabled,
    // The recovery banner shows the disable confirmation in the surface that offered the
    // action, so this only performs the change the user confirmed.
    disableRecoveredAgent: (agentId, acceptedActiveWorkInterruption) =>
      setAgentEnabled(agentId, false, acceptedActiveWorkInterruption),
    setMcpServerEnabled: (id, enabled) => {
      void setMcpServerEnabledThroughBackend(agentSettingsContext(), id, enabled)
        .catch((error) => dispatch({ type: "settings:error", tab: "mcp", message: safeErrorMessage(error) }));
    },
    setComposerSubmitShortcut: (shortcut) => {
      const nextPreferences = { composer_submit_shortcut: shortcut };
      setPreferences(nextPreferences);
      dispatch({ type: "settings:preferences", preferences: nextPreferences });
      const connection = settingsConnection();
      if (!connection) {
        dispatch({ type: "settings:error", tab: "general", message: appServerRequiredMessage() });
        return;
      }
      void connection.request(SETTINGS_UPDATE_PREFERENCES, {
        preferences: { composerSubmitShortcut: protocolComposerSubmitShortcut(shortcut) },
      })
        .then((result) => {
          const preferences = mapProtocolAppPreferences(result);
          setPreferences(preferences);
          dispatch({ type: "settings:preferences", preferences });
        })
        .catch((error) => dispatch({ type: "settings:error", tab: "general", message: safeErrorMessage(error) }));
    },
    updateCustomAgentMetadata: async (payload, acceptedActiveWorkInterruption) => {
      dispatch({ type: "settings:start" });
      try {
        const outcome = await updateCustomAgentMetadataThroughBackend(
          agentSettingsContext(),
          payload,
          acceptedActiveWorkInterruption,
        );
        if (outcome.kind === "unavailable") {
          dispatch({ type: "settings:error", tab: "agents", message: appServerRequiredMessage() });
        }
        return outcome;
      } catch (error) {
        dispatch({ type: "settings:error", tab: "agents", message: safeErrorMessage(error) });
        return { kind: "failed" };
      }
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
