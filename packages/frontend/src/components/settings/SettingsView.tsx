import { AlertTriangle, RefreshCcw } from "lucide-react";
import { useEffect, useState } from "react";
import type { KeyboardEvent } from "react";
import type {
  AppPreferencesRecord,
  ComposerSubmitShortcut,
  CustomAgentCreateParams,
  CustomAgentMetadataUpdateParams,
  CustomAgentReplaceParams,
  AgentSettingsRecord,
  SettingsTabId,
  RuntimeSettingsResult,
} from "@openaide/app-shell-contracts";
import type { SettingsState } from "../../state/store";
import { AgentSettingsTab } from "./AgentSettingsTab";
import { GeneralSettingsTab } from "./GeneralSettingsTab";
import { McpSettingsTab, SkillsSettingsTab } from "./NonAgentSettingsTabs";
import { SettingsSkeleton } from "./settingsPresentation";

const tabs: Array<{ id: SettingsTabId; label: string }> = [
  { id: "agents", label: "Agents" },
  { id: "mcp", label: "MCP" },
  { id: "skills", label: "Skills" },
  { id: "common", label: "General" },
];

export function SettingsView({
  onAuthenticate,
  onCreateCustomAgent,
  onDeleteCustomAgent,
  onReplaceCustomAgent,
  onSetAgentEnabled,
  onUpdateCustomAgentMetadata,
  onUnlockDeveloperSettings,
  onRefresh,
  onSetAcpTrace,
  onSetComposerSubmitShortcut,
  onSelectTab,
  preferences,
  state,
}: {
  onAuthenticate: (agentId: string, methodId: string) => void;
  onCreateCustomAgent: (params: CustomAgentCreateParams) => void;
  onDeleteCustomAgent: (agentId: string) => void;
  onReplaceCustomAgent: (params: CustomAgentReplaceParams) => void;
  onSetAgentEnabled: (agentId: string, enabled: boolean) => void;
  onUpdateCustomAgentMetadata: (params: CustomAgentMetadataUpdateParams) => void;
  onUnlockDeveloperSettings: () => void;
  onRefresh: () => void;
  onSetAcpTrace: (enabled: boolean) => void;
  onSetComposerSubmitShortcut: (shortcut: ComposerSubmitShortcut) => void;
  onSelectTab: (tab: SettingsTabId) => void;
  preferences: AppPreferencesRecord;
  state: SettingsState;
}) {
  const visibleTabs = tabs.filter((tab) => (state.availableTabs ?? ["agents", "common"]).includes(tab.id));
  const activeTab = visibleTabs.some((tab) => tab.id === state.activeTab) ? state.activeTab : visibleTabs[0]?.id ?? "agents";
  const selectedIndex = visibleTabs.findIndex((tab) => tab.id === activeTab);
  const busy = state.loading || state.mcpServersLoading || state.skillsLoading;
  const showAgentSkeleton = activeTab === "agents" && state.loading && !state.agentDetails;
  const [developerUnlockClicks, setDeveloperUnlockClicks] = useState(0);
  const [developerSettingsUnlocked, setDeveloperSettingsUnlocked] = useState(false);
  const selectTab = (tab: SettingsTabId, focus = false) => {
    onSelectTab(tab);
    if (focus) {
      window.requestAnimationFrame(() => document.getElementById(settingsTabId(tab))?.focus());
    }
  };
  const onTabKeyDown = (event: KeyboardEvent<HTMLButtonElement>, index: number) => {
    let nextIndex: number | undefined;
    if (event.key === "ArrowRight" || event.key === "ArrowDown") nextIndex = (index + 1) % visibleTabs.length;
    if (event.key === "ArrowLeft" || event.key === "ArrowUp") nextIndex = (index - 1 + visibleTabs.length) % visibleTabs.length;
    if (event.key === "Home") nextIndex = 0;
    if (event.key === "End") nextIndex = visibleTabs.length - 1;
    if (nextIndex === undefined) return;
    event.preventDefault();
    selectTab(visibleTabs[nextIndex].id, true);
  };
  const onTitleClick = () => {
    if (developerSettingsUnlocked) return;
    const nextCount = developerUnlockClicks + 1;
    if (nextCount >= 7) {
      setDeveloperUnlockClicks(0);
      setDeveloperSettingsUnlocked(true);
      onUnlockDeveloperSettings();
      return;
    }
    setDeveloperUnlockClicks(nextCount);
  };
  useEffect(() => {
    if (typeof window === "undefined" || typeof document === "undefined") return;
    const frame = window.requestAnimationFrame(() => document.getElementById(settingsTabId(activeTab))?.focus());
    return () => window.cancelAnimationFrame(frame);
  }, [activeTab]);

  return (
    <section className="settings-view" aria-label="Settings">
      <header className="settings-header">
        <div className="settings-header-copy">
          <h1>
            <button className="settings-title-button" onClick={onTitleClick} type="button">
              Settings
            </button>
          </h1>
          <p>Agent and app configuration.</p>
        </div>
        <button disabled={busy} onClick={onRefresh} type="button">
          <RefreshCcw size={13} />
          Refresh all
        </button>
      </header>
      <div className="settings-body">
        <div className="settings-tabs" role="tablist" aria-label="Settings sections">
          {visibleTabs.map((tab, index) => (
            <button
              key={tab.id}
              id={settingsTabId(tab.id)}
              aria-controls={settingsPanelId(tab.id)}
              aria-selected={activeTab === tab.id}
              autoFocus={index === selectedIndex}
              className={activeTab === tab.id ? "selected" : ""}
              onClick={() => selectTab(tab.id)}
              onKeyDown={(event) => onTabKeyDown(event, index)}
              role="tab"
              tabIndex={index === selectedIndex ? 0 : -1}
              type="button"
            >
              {tab.label}
            </button>
          ))}
        </div>
        <div className="settings-content">
          {state.error ? (
            <section className="settings-error" aria-label="Settings error">
              <AlertTriangle size={14} />
              <span>{state.error}</span>
            </section>
          ) : null}
          {showAgentSkeleton ? (
            <SettingsSkeleton />
          ) : (
            <SettingsTabContent
              agents={state.agentDetails ?? []}
              authPending={state.loading}
              deletedAgentId={state.deletedAgentId}
              onAuthenticate={onAuthenticate}
              onCreateCustomAgent={onCreateCustomAgent}
              onDeleteCustomAgent={onDeleteCustomAgent}
              onReplaceCustomAgent={onReplaceCustomAgent}
              onSetAcpTrace={onSetAcpTrace}
              onSetAgentEnabled={onSetAgentEnabled}
              onUpdateCustomAgentMetadata={onUpdateCustomAgentMetadata}
              onSetComposerSubmitShortcut={onSetComposerSubmitShortcut}
              preferences={preferences}
              developerSettingsUnlocked={developerSettingsUnlocked}
              savedAgentId={state.savedAgentId}
              runtimeSettings={state.runtimeSettings}
              settingsState={state}
              tab={activeTab}
            />
          )}
        </div>
      </div>
    </section>
  );
}

function SettingsTabContent({
  onAuthenticate,
  onCreateCustomAgent,
  onDeleteCustomAgent,
  onReplaceCustomAgent,
  onSetAgentEnabled,
  onSetAcpTrace,
  onSetComposerSubmitShortcut,
  onUpdateCustomAgentMetadata,
  authPending,
  agents,
  developerSettingsUnlocked,
  preferences,
  savedAgentId,
  deletedAgentId,
  runtimeSettings,
  settingsState,
  tab,
}: {
  authPending: boolean;
  agents: AgentSettingsRecord[];
  onAuthenticate: (agentId: string, methodId: string) => void;
  onCreateCustomAgent: (params: CustomAgentCreateParams) => void;
  onDeleteCustomAgent: (agentId: string) => void;
  onReplaceCustomAgent: (params: CustomAgentReplaceParams) => void;
  onSetAgentEnabled: (agentId: string, enabled: boolean) => void;
  onSetAcpTrace: (enabled: boolean) => void;
  onSetComposerSubmitShortcut: (shortcut: ComposerSubmitShortcut) => void;
  onUpdateCustomAgentMetadata: (params: CustomAgentMetadataUpdateParams) => void;
  deletedAgentId?: string;
  developerSettingsUnlocked: boolean;
  preferences: AppPreferencesRecord;
  savedAgentId?: string;
  runtimeSettings?: RuntimeSettingsResult;
  settingsState: SettingsState;
  tab: SettingsTabId;
}) {
  return (
    <div
      className={`settings-tab-panel ${tab === "agents" ? "wide" : "narrow"}`}
      id={settingsPanelId(tab)}
      role="tabpanel"
      aria-labelledby={settingsTabId(tab)}
    >
      {tab === "agents" ? (
        <AgentSettingsTab
          agents={agents}
          authPending={authPending}
          deletedAgentId={deletedAgentId}
          onAuthenticate={onAuthenticate}
          onCreateCustomAgent={onCreateCustomAgent}
          onDeleteCustomAgent={onDeleteCustomAgent}
          onReplaceCustomAgent={onReplaceCustomAgent}
          onSetAgentEnabled={onSetAgentEnabled}
          onUpdateCustomAgentMetadata={onUpdateCustomAgentMetadata}
          savedAgentId={savedAgentId}
        />
      ) : null}
      {tab === "common" ? (
        <GeneralSettingsTab
          developerSettingsUnlocked={developerSettingsUnlocked}
          onSetAcpTrace={onSetAcpTrace}
          onSetComposerSubmitShortcut={onSetComposerSubmitShortcut}
          preferences={preferences}
          runtimeSettings={runtimeSettings}
        />
      ) : null}
      {tab === "mcp" ? (
        <McpSettingsTab
          availability={settingsState.mcpServersAvailability}
          error={settingsState.mcpServersError}
          loading={settingsState.mcpServersLoading}
          servers={settingsState.mcpServers}
        />
      ) : null}
      {tab === "skills" ? (
        <SkillsSettingsTab
          availability={settingsState.skillsAvailability}
          error={settingsState.skillsError}
          loading={settingsState.skillsLoading}
          skills={settingsState.skills}
        />
      ) : null}
    </div>
  );
}

function settingsTabId(tab: SettingsTabId) {
  return `settings-tab-${tab}`;
}

function settingsPanelId(tab: SettingsTabId) {
  return `settings-panel-${tab}`;
}
