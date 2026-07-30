import {
  AlertTriangle,
  ArrowLeft,
  Bot,
  FolderGit2,
  Network,
  RefreshCcw,
  Search,
  SlidersHorizontal,
  Sparkles,
} from "lucide-react";
import { useEffect, useState } from "react";
import type { KeyboardEvent } from "react";
import type {
  AppPreferencesRecord,
  AppTheme,
  ComposerSubmitShortcut,
  CustomAgentCreateParams,
  CustomAgentMetadataUpdateParams,
  CustomAgentReplaceParams,
  AgentSettingsRecord,
  SettingsTabId,
  RuntimeSettingsResult,
  SkillSettingsDetails,
} from "@openaide/app-shell-contracts";
import type { SettingsState } from "../../state/store";
import type { ProjectOption } from "../../state/composerOptions";
import type { McpServerDefinition } from "@openaide/app-server-client";
import type {
  WorktreeRepositorySnapshot,
  WorktreeSummary,
} from "@openaide/app-server-client";
import type { McpServerSaveInput } from "../../intents/mcpSettingsIntents";
import type { NewTaskViewIntents } from "../NewTaskView";
import { AgentSettingsTab } from "./AgentSettingsTab";
import { GeneralSettingsTab } from "./GeneralSettingsTab";
import { SkillsSettingsTab } from "./NonAgentSettingsTabs";
import { McpSettingsTab } from "./McpSettingsTab";
import { WorktreesSettingsTab } from "./WorktreesSettingsTab";
import { SettingsSkeleton } from "./settingsPresentation";
import type { DesktopNotificationSettings } from "../../shells/webTaskNotifications";
import type { AgentRecoveryActions } from "../AgentRecovery";
import { AppSidebarFrame } from "../AppSidebarFrame";

const tabs: Array<{
  group: "App" | "Agent work" | "Projects";
  icon: typeof Bot;
  id: SettingsTabId;
  label: string;
}> = [
  { group: "App", icon: SlidersHorizontal, id: "common", label: "General" },
  { group: "Agent work", icon: Bot, id: "agents", label: "Agents" },
  { group: "Agent work", icon: Network, id: "mcp", label: "MCP Servers" },
  { group: "Agent work", icon: Sparkles, id: "skills", label: "Skills" },
  { group: "Projects", icon: FolderGit2, id: "worktrees", label: "Worktrees" },
];

export function SettingsView({
  desktopNotifications,
  onAuthenticate,
  onBackToApp,
  onCreateCustomAgent,
  onDeleteCustomAgent,
  onDeleteMcpServer = () => undefined,
  onGetMcpServerDetails = async () => { throw new Error("MCP settings require the App Server."); },
  onGetSkillDetails,
  onReplaceCustomAgent,
  onResetTaskHistory,
  onSetAgentEnabled,
  onSetMcpServerEnabled = () => undefined,
  onSaveMcpServer = () => undefined,
  onUpdateCustomAgentMetadata,
  onUnlockDeveloperSettings,
  onRefresh,
  onNewTaskInWorktree,
  onSetAcpTrace,
  onSetComposerSubmitShortcut,
  onSelectTab,
  onSetDesktopNotifications,
  onSetTheme,
  preferences,
  preferredAgentId,
  projects = [],
  recoveryActions,
  state,
  worktreeIntents,
  worktreeRepositories = {},
}: {
  desktopNotifications?: DesktopNotificationSettings;
  onAuthenticate: (agentId: string, methodId: string, values?: Record<string, string>) => void | Promise<boolean>;
  onBackToApp?: () => void;
  onCreateCustomAgent: (params: CustomAgentCreateParams) => void;
  onDeleteCustomAgent: (agentId: string) => void;
  onDeleteMcpServer?: (server: McpServerDefinition) => void;
  onGetMcpServerDetails?: (id: string) => Promise<McpServerDefinition>;
  onGetSkillDetails?: (id: string) => Promise<SkillSettingsDetails>;
  onNewTaskInWorktree?: (project: ProjectOption, worktree: WorktreeSummary) => void;
  onReplaceCustomAgent: (params: CustomAgentReplaceParams) => void;
  onResetTaskHistory?: () => Promise<void>;
  onSetAgentEnabled: (agentId: string, enabled: boolean) => void;
  onSetMcpServerEnabled?: (id: string, enabled: boolean) => void;
  onSaveMcpServer?: (input: McpServerSaveInput) => void;
  onUpdateCustomAgentMetadata: (params: CustomAgentMetadataUpdateParams) => void;
  onUnlockDeveloperSettings: () => void;
  onRefresh: () => void;
  onSetAcpTrace: (enabled: boolean) => void;
  onSetComposerSubmitShortcut: (shortcut: ComposerSubmitShortcut) => void;
  onSelectTab: (tab: SettingsTabId) => void;
  onSetDesktopNotifications?: (enabled: boolean) => void | Promise<void>;
  onSetTheme?: (theme: AppTheme) => void;
  preferences: AppPreferencesRecord;
  preferredAgentId?: string;
  projects?: ProjectOption[];
  recoveryActions?: AgentRecoveryActions;
  state: SettingsState;
  worktreeIntents?: NewTaskViewIntents;
  worktreeRepositories?: Record<string, WorktreeRepositorySnapshot>;
}) {
  const visibleTabs = tabs.filter((tab) => (
    tab.id === "worktrees"
    || (state.availableTabs ?? ["agents", "common"]).includes(tab.id)
  ));
  const activeTab = visibleTabs.some((tab) => tab.id === state.activeTab) ? state.activeTab : visibleTabs[0]?.id ?? "agents";
  const busy = state.loading || state.mcpServersLoading || state.skillsLoading;
  const showAgentSkeleton = activeTab === "agents" && state.loading && !state.agentDetails;
  const [navigationQuery, setNavigationQuery] = useState("");
  const [mobileIndexOpen, setMobileIndexOpen] = useState(isNarrowSettingsViewport);
  const [developerUnlockClicks, setDeveloperUnlockClicks] = useState(0);
  const [developerSettingsUnlocked, setDeveloperSettingsUnlocked] = useState(false);
  const normalizedQuery = navigationQuery.trim().toLowerCase();
  const navigationTabs = visibleTabs.filter((tab) => (
    !normalizedQuery || `${tab.group} ${tab.label}`.toLowerCase().includes(normalizedQuery)
  ));
  const selectTab = (tab: SettingsTabId, focus = false) => {
    onSelectTab(tab);
    setMobileIndexOpen(false);
    if (focus) {
      window.requestAnimationFrame(() => document.getElementById(settingsTabId(tab))?.focus());
    }
  };
  const onTabKeyDown = (event: KeyboardEvent<HTMLButtonElement>, index: number) => {
    let nextIndex: number | undefined;
    if (event.key === "ArrowRight" || event.key === "ArrowDown") nextIndex = (index + 1) % navigationTabs.length;
    if (event.key === "ArrowLeft" || event.key === "ArrowUp") nextIndex = (index - 1 + navigationTabs.length) % navigationTabs.length;
    if (event.key === "Home") nextIndex = 0;
    if (event.key === "End") nextIndex = navigationTabs.length - 1;
    if (nextIndex === undefined) return;
    event.preventDefault();
    selectTab(navigationTabs[nextIndex].id, true);
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
    if (mobileIndexOpen || typeof window === "undefined" || typeof document === "undefined") return;
    const frame = window.requestAnimationFrame(() => document.getElementById(settingsTabId(activeTab))?.focus());
    return () => window.cancelAnimationFrame(frame);
  }, [activeTab, mobileIndexOpen]);

  const sidebar = (
    <aside className="settings-sidebar">
        <div className="settings-sidebar-heading">
          {onBackToApp ? (
            <button
              aria-label="Back to app"
              className="settings-back-to-app"
              onClick={onBackToApp}
              type="button"
            >
              <ArrowLeft size={14} /> Back to app
            </button>
          ) : null}
          <header className="settings-header">
            <span>
              <button className="settings-title-button" onClick={onTitleClick} type="button">
                Settings
              </button>
              <small>OpenAIDE</small>
            </span>
            <span className="settings-header-actions">
              <button
                aria-label="Refresh settings"
                disabled={busy}
                onClick={onRefresh}
                title="Refresh settings"
                type="button"
              >
                <RefreshCcw size={13} />
              </button>
            </span>
          </header>
        </div>
        <label className="settings-search">
          <Search aria-hidden="true" size={14} />
          <input
            aria-label="Search Settings"
            onChange={(event) => setNavigationQuery(event.currentTarget.value)}
            placeholder="Search Settings"
            type="search"
            value={navigationQuery}
          />
        </label>
        <nav className="settings-tabs" role="tablist" aria-label="Settings sections">
          {(["App", "Agent work", "Projects"] as const).map((group) => {
            const groupTabs = navigationTabs.filter((tab) => tab.group === group);
            if (!groupTabs.length) return null;
            return (
              <section key={group}>
                <h2>{group}</h2>
                {groupTabs.map((tab) => {
                  const index = navigationTabs.findIndex((candidate) => candidate.id === tab.id);
                  const Icon = tab.icon;
                  return (
                    <button
                      key={tab.id}
                      id={settingsTabId(tab.id)}
                      aria-controls={settingsPanelId(tab.id)}
                      aria-selected={activeTab === tab.id}
                      autoFocus={activeTab === tab.id}
                      className={activeTab === tab.id ? "selected" : ""}
                      onClick={() => selectTab(tab.id)}
                      onKeyDown={(event) => onTabKeyDown(event, index)}
                      role="tab"
                      tabIndex={tab.id === activeTab ? 0 : -1}
                      type="button"
                    >
                      <Icon aria-hidden="true" size={15} />
                      <span>{tab.label}</span>
                    </button>
                  );
                })}
              </section>
            );
          })}
          {!navigationTabs.length ? <p>No settings found.</p> : null}
        </nav>
      </aside>
  );
  return (
    <AppSidebarFrame aria-label="Settings" className="settings-view" sidebar={sidebar}>
      <section className={`settings-mobile-index ${mobileIndexOpen ? "open" : ""}`}>
        {onBackToApp ? (
          <button
            aria-label="Back to app"
            className="settings-back-to-app"
            onClick={onBackToApp}
            type="button"
          >
            <ArrowLeft size={14} /> Back to app
          </button>
        ) : null}
        <header>
          <span>
            <strong>Settings</strong>
            <small>OpenAIDE</small>
          </span>
        </header>
        <label className="settings-search">
          <Search aria-hidden="true" size={14} />
          <input
            aria-label="Search Settings pages"
            onChange={(event) => setNavigationQuery(event.currentTarget.value)}
            placeholder="Search Settings"
            type="search"
            value={navigationQuery}
          />
        </label>
        <nav aria-label="Settings pages">
          {(["App", "Agent work", "Projects"] as const).map((group) => {
            const groupTabs = navigationTabs.filter((tab) => tab.group === group);
            if (!groupTabs.length) return null;
            return (
              <section key={group}>
                <h2>{group}</h2>
                {groupTabs.map((tab) => {
                  const Icon = tab.icon;
                  return (
                    <button key={tab.id} onClick={() => selectTab(tab.id)} type="button">
                      <Icon aria-hidden="true" size={16} />
                      <span>{tab.label}</span>
                    </button>
                  );
                })}
              </section>
            );
          })}
        </nav>
      </section>
      <div className={`settings-content ${mobileIndexOpen ? "mobile-index-open" : ""}`}>
        <header className="settings-page-heading">
          <button className="mobile-settings-index-button" onClick={() => setMobileIndexOpen(true)} type="button">
            <ArrowLeft size={14} /> Settings
          </button>
          <span>{tabs.find((tab) => tab.id === activeTab)?.group}</span>
          <h1>{tabs.find((tab) => tab.id === activeTab)?.label}</h1>
        </header>
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
            desktopNotifications={desktopNotifications}
            agents={state.agentDetails ?? []}
            authPending={state.loading}
            deletedAgentId={state.deletedAgentId}
            onAuthenticate={onAuthenticate}
            onCreateCustomAgent={onCreateCustomAgent}
            onDeleteCustomAgent={onDeleteCustomAgent}
            onDeleteMcpServer={onDeleteMcpServer}
            onGetMcpServerDetails={onGetMcpServerDetails}
            onGetSkillDetails={onGetSkillDetails}
            onNewTaskInWorktree={onNewTaskInWorktree}
            onReplaceCustomAgent={onReplaceCustomAgent}
            onResetTaskHistory={onResetTaskHistory}
            onSetAcpTrace={onSetAcpTrace}
            onSetAgentEnabled={onSetAgentEnabled}
            onSetMcpServerEnabled={onSetMcpServerEnabled}
            onSaveMcpServer={onSaveMcpServer}
            onUpdateCustomAgentMetadata={onUpdateCustomAgentMetadata}
            onSetComposerSubmitShortcut={onSetComposerSubmitShortcut}
            onSetDesktopNotifications={onSetDesktopNotifications}
            onSetTheme={onSetTheme}
            preferences={preferences}
            preferredAgentId={preferredAgentId}
            projects={projects}
            recoveryActions={recoveryActions}
            developerSettingsUnlocked={developerSettingsUnlocked}
            saveError={state.error}
            savedAgentId={state.savedAgentId}
            runtimeSettings={state.runtimeSettings}
            settingsState={state}
            tab={activeTab}
            worktreeIntents={worktreeIntents}
            worktreeRepositories={worktreeRepositories}
          />
        )}
      </div>
    </AppSidebarFrame>
  );
}

function isNarrowSettingsViewport() {
  if (typeof window === "undefined") return false;
  if (typeof window.matchMedia === "function") {
    return window.matchMedia("(max-width: 760px)").matches;
  }
  return window.innerWidth <= 760;
}

function SettingsTabContent({
  desktopNotifications,
  onAuthenticate,
  onCreateCustomAgent,
  onDeleteCustomAgent,
  onDeleteMcpServer,
  onGetMcpServerDetails,
  onGetSkillDetails,
  onNewTaskInWorktree,
  onReplaceCustomAgent,
  onResetTaskHistory,
  onSetAgentEnabled,
  onSetMcpServerEnabled,
  onSaveMcpServer,
  onSetAcpTrace,
  onSetComposerSubmitShortcut,
  onSetDesktopNotifications,
  onSetTheme,
  onUpdateCustomAgentMetadata,
  authPending,
  agents,
  developerSettingsUnlocked,
  preferences,
  preferredAgentId,
  projects,
  recoveryActions,
  saveError,
  savedAgentId,
  deletedAgentId,
  runtimeSettings,
  settingsState,
  tab,
  worktreeIntents,
  worktreeRepositories,
}: {
  desktopNotifications?: DesktopNotificationSettings;
  authPending: boolean;
  agents: AgentSettingsRecord[];
  onAuthenticate: (agentId: string, methodId: string, values?: Record<string, string>) => void | Promise<boolean>;
  onCreateCustomAgent: (params: CustomAgentCreateParams) => void;
  onDeleteCustomAgent: (agentId: string) => void;
  onDeleteMcpServer: (server: McpServerDefinition) => void;
  onGetMcpServerDetails: (id: string) => Promise<McpServerDefinition>;
  onGetSkillDetails?: (id: string) => Promise<SkillSettingsDetails>;
  onNewTaskInWorktree?: (project: ProjectOption, worktree: WorktreeSummary) => void;
  onReplaceCustomAgent: (params: CustomAgentReplaceParams) => void;
  onResetTaskHistory?: () => Promise<void>;
  onSetAgentEnabled: (agentId: string, enabled: boolean) => void;
  onSetMcpServerEnabled: (id: string, enabled: boolean) => void;
  onSaveMcpServer: (input: McpServerSaveInput) => void;
  onSetAcpTrace: (enabled: boolean) => void;
  onSetComposerSubmitShortcut: (shortcut: ComposerSubmitShortcut) => void;
  onSetDesktopNotifications?: (enabled: boolean) => void | Promise<void>;
  onSetTheme?: (theme: AppTheme) => void;
  onUpdateCustomAgentMetadata: (params: CustomAgentMetadataUpdateParams) => void;
  deletedAgentId?: string;
  developerSettingsUnlocked: boolean;
  preferences: AppPreferencesRecord;
  preferredAgentId?: string;
  projects: ProjectOption[];
  recoveryActions?: AgentRecoveryActions;
  saveError?: string;
  savedAgentId?: string;
  runtimeSettings?: RuntimeSettingsResult;
  settingsState: SettingsState;
  tab: SettingsTabId;
  worktreeIntents?: NewTaskViewIntents;
  worktreeRepositories: Record<string, WorktreeRepositorySnapshot>;
}) {
  return (
    <div
      className={`settings-tab-panel ${tab === "agents" || tab === "worktrees" ? "wide" : "narrow"}`}
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
          preferredAgentId={preferredAgentId}
          recoveryActions={recoveryActions}
          onCreateCustomAgent={onCreateCustomAgent}
          onDeleteCustomAgent={onDeleteCustomAgent}
          onReplaceCustomAgent={onReplaceCustomAgent}
          onSetAgentEnabled={onSetAgentEnabled}
          onUpdateCustomAgentMetadata={onUpdateCustomAgentMetadata}
          saveError={saveError}
          savedAgentId={savedAgentId}
        />
      ) : null}
      {tab === "common" ? (
        <GeneralSettingsTab
          developerSettingsUnlocked={developerSettingsUnlocked}
          desktopNotifications={desktopNotifications}
          onSetAcpTrace={onSetAcpTrace}
          onResetTaskHistory={onResetTaskHistory}
          onSetComposerSubmitShortcut={onSetComposerSubmitShortcut}
          onSetDesktopNotifications={onSetDesktopNotifications}
          onSetTheme={onSetTheme}
          preferences={preferences}
          runtimeSettings={runtimeSettings}
        />
      ) : null}
      {tab === "mcp" ? (
        <McpSettingsTab
          availability={settingsState.mcpServersAvailability}
          error={settingsState.mcpServersError}
          loading={settingsState.mcpServersLoading}
          onDeleteServer={onDeleteMcpServer}
          onLoadServer={onGetMcpServerDetails}
          onSaveServer={onSaveMcpServer}
          onSetEnabled={onSetMcpServerEnabled}
          projects={projects}
          servers={settingsState.mcpServers}
        />
      ) : null}
      {tab === "skills" ? (
        <SkillsSettingsTab
          availability={settingsState.skillsAvailability}
          error={settingsState.skillsError}
          loading={settingsState.skillsLoading}
          onLoadSkill={onGetSkillDetails}
          skills={settingsState.skills}
        />
      ) : null}
      {tab === "worktrees" && worktreeIntents && onNewTaskInWorktree ? (
        <WorktreesSettingsTab
          intents={worktreeIntents}
          onNewTask={onNewTaskInWorktree}
          projects={projects}
          repositories={worktreeRepositories}
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
