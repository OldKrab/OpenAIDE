import { ArrowLeft, ChevronRight, Search, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import type {
  AppPreferencesRecord,
  ComposerSubmitShortcut,
  CustomAgentCreateParams,
  CustomAgentMetadataUpdateParams,
  CustomAgentReplaceParams,
  SettingsTabId,
  SkillSettingsDetails,
} from "@openaide/app-shell-contracts";
import type { BackendConnection, McpServerDefinition, WorktreeRepositorySnapshot, WorktreeSummary } from "@openaide/app-server-client";
import type { AgentDisableOutcome } from "../../intents/agentSettingsIntents";
import type { McpServerSaveInput } from "../../intents/mcpSettingsIntents";
import type { RemoteDeviceIntents } from "../../intents/remoteDeviceIntents";
import { currentFrontendShell, type FrontendShellAppearance } from "../../services/frontendShell";
import type { DesktopNotificationSettings } from "../../shells/webTaskNotifications";
import type { ProjectOption } from "../../state/composerOptions";
import type { SettingsState } from "../../state/store";
import type { AgentRecoveryActions } from "../AgentRecovery";
import { AppSidebarFrame } from "../AppSidebarFrame";
import { useBackNavigation } from "../useBackNavigation";
import { agentLeftLaunching } from "./agentSettingsModel";
import { useAgentsSection } from "./AgentsSection";
import { useGeneralSection } from "./GeneralSection";
import { useMcpSection } from "./McpSection";
import { phoneSections, usePhoneConnection } from "./phoneConnection";
import { useRemoteSection } from "./RemoteSection";
import { InfoHint, SettingsGroupView, SettingsInlineError, mark } from "./settingsKit";
import { filterSections, infoMatches, normalizeNeedle, type SettingsSection } from "./settingsModel";
import { useSkillsSection } from "./SkillsSection";
import { useTroubleshootingSection } from "./TroubleshootingSection";
import { useWorktreesSection } from "./WorktreesSection";
import type { WorktreeSettingsIntents } from "./worktreeSettingsModel";

/** Sections whose lists are App Server projections that can change while Settings is open. */
const projectedSections: SettingsTabId[] = ["agents", "mcp", "skills"];

export type SettingsViewProps = {
  appearance?: FrontendShellAppearance;
  backendConnection?: Pick<BackendConnection, "request">;
  desktopNotifications?: DesktopNotificationSettings;
  /** Present when this client can pair and remove Remote Devices. */
  deviceIntents?: RemoteDeviceIntents;
  frameHeader?: ReactNode;
  frameHeaderPlacement?: "overlay" | "row";
  onAuthenticate: (agentId: string, methodId: string, values?: Record<string, string>) => void | Promise<boolean>;
  onBackToApp?: () => void;
  onCancelAuthentication?: (agentId: string) => void | Promise<void>;
  onCreateCustomAgent: (params: CustomAgentCreateParams) => void;
  onDeleteCustomAgent: (agentId: string) => void;
  onDeleteMcpServer?: (server: McpServerDefinition) => void;
  onDismissError?: () => void;
  onGetMcpServerDetails?: (id: string) => Promise<McpServerDefinition>;
  onGetSkillDetails?: (id: string) => Promise<SkillSettingsDetails>;
  onLogout?: (agentId: string) => boolean | void | Promise<boolean | void>;
  onNewTaskInWorktree?: (project: ProjectOption, worktree: WorktreeSummary) => void;
  onRefresh: () => void;
  onReplaceCustomAgent: (params: CustomAgentReplaceParams) => void;
  onResetTaskHistory?: () => Promise<void>;
  onSaveMcpServer?: (input: McpServerSaveInput) => void;
  onSelectTab: (tab: SettingsTabId) => void;
  onSetAcpTrace: (enabled: boolean) => void;
  onSetAgentEnabled: (agentId: string, enabled: boolean, acceptedActiveWorkInterruption?: boolean) => Promise<AgentDisableOutcome>;
  onSetComposerSubmitShortcut: (shortcut: ComposerSubmitShortcut) => void;
  onSetDesktopNotifications?: (enabled: boolean) => void | Promise<void>;
  onSetMcpServerEnabled?: (id: string, enabled: boolean) => void;
  onUpdateCustomAgentMetadata: (params: CustomAgentMetadataUpdateParams, acceptedActiveWorkInterruption?: boolean) => Promise<AgentDisableOutcome>;
  preferences: AppPreferencesRecord;
  preferredAgentId?: string;
  projects?: ProjectOption[];
  recoveryActions?: AgentRecoveryActions;
  state: SettingsState;
  supportExportRequestKey?: string;
  worktreeIntents?: WorktreeSettingsIntents;
  worktreeRepositories?: Record<string, WorktreeRepositorySnapshot>;
};

const noRepositories: Record<string, WorktreeRepositorySnapshot> = {};
const noProjects: ProjectOption[] = [];

/**
 * The Settings shell: a list of sections, a search over everything they
 * declare, and the page of the section in view. Each section is a hook that
 * returns its declaration, so this file knows no setting by name (ADR 0065).
 */
export function SettingsView({
  appearance = currentFrontendShell()?.appearance,
  backendConnection,
  desktopNotifications,
  deviceIntents,
  frameHeader,
  frameHeaderPlacement,
  onAuthenticate,
  onBackToApp,
  onCancelAuthentication,
  onCreateCustomAgent,
  onDeleteCustomAgent,
  onDeleteMcpServer = () => undefined,
  onDismissError,
  onGetMcpServerDetails = async () => { throw new Error("MCP settings require the App Server."); },
  onGetSkillDetails,
  onLogout,
  onNewTaskInWorktree,
  onRefresh,
  onReplaceCustomAgent,
  onResetTaskHistory,
  onSaveMcpServer = () => undefined,
  onSelectTab,
  onSetAcpTrace,
  onSetAgentEnabled,
  onSetComposerSubmitShortcut,
  onSetDesktopNotifications,
  onSetMcpServerEnabled = () => undefined,
  onUpdateCustomAgentMetadata,
  preferences,
  preferredAgentId,
  projects = noProjects,
  recoveryActions,
  state,
  supportExportRequestKey,
  worktreeIntents,
  worktreeRepositories = noRepositories,
}: SettingsViewProps) {
  const availableTabs = state.availableTabs ?? ["agents", "general", "troubleshooting"];
  const phone = usePhoneConnection(currentFrontendShell()?.connectionSettings);
  const [query, setQuery] = useState("");
  // A phone starts on the index unless the route already names the section to show.
  const [indexOpen, setIndexOpen] = useState(() => isNarrowSettingsViewport() && !routeNamesSection());
  const searchRef = useRef<HTMLInputElement>(null);
  const pageRef = useRef<HTMLElement>(null);

  const general = useGeneralSection({
    appearance,
    desktopNotifications,
    onSetComposerSubmitShortcut,
    onSetDesktopNotifications,
    phone,
    preferences,
    replacementImpact: deviceIntents?.replacementImpact,
  });
  const agents = useAgentsSection({
    agents: state.loading && !state.agentDetails ? undefined : state.agentDetails ?? [],
    available: availableTabs.includes("agents"),
    deletedAgentId: state.deletedAgentId,
    onAuthenticate,
    onCancelAuthentication,
    onCreateCustomAgent,
    onDeleteCustomAgent,
    onLogout,
    onReplaceCustomAgent,
    onSetAgentEnabled,
    onUpdateCustomAgentMetadata,
    preferredAgentId,
    recoveryActions,
    saveError: state.error,
    savedAgentId: state.savedAgentId,
  });
  const mcp = useMcpSection({
    available: availableTabs.includes("mcp"),
    availability: state.mcpServersAvailability,
    error: state.mcpServersError,
    loading: state.mcpServersLoading,
    onDeleteServer: onDeleteMcpServer,
    onLoadServer: onGetMcpServerDetails,
    onSaveServer: onSaveMcpServer,
    onSetEnabled: onSetMcpServerEnabled,
    projects,
    servers: state.mcpServers,
  });
  const skills = useSkillsSection({
    available: availableTabs.includes("skills"),
    availability: state.skillsAvailability,
    error: state.skillsError,
    loading: state.skillsLoading,
    onLoadSkill: onGetSkillDetails,
    skills: state.skills,
  });
  const worktrees = useWorktreesSection({
    active: state.activeTab === "worktrees",
    intents: worktreeIntents,
    onNewTask: onNewTaskInWorktree,
    projects,
    repositories: worktreeRepositories,
  });
  const remote = useRemoteSection({ deviceIntents, devices: state.devices, phone });
  const troubleshooting = useTroubleshootingSection({
    backendConnection,
    onResetTaskHistory,
    onSetAcpTrace,
    phone,
    runtimeSettings: state.runtimeSettings,
    supportExportRequestKey,
  });

  const sections = [general, agents, mcp, skills, worktrees, remote, troubleshooting]
    .filter((section): section is SettingsSection => Boolean(section));
  const activeId = sections.some((section) => section.id === state.activeTab) ? state.activeTab : sections[0].id;
  const needle = normalizeNeedle(query);
  const filtered = filterSections(sections, needle);
  const current = filtered.find((section) => section.id === activeId) ?? filtered[0];
  // The page of one item stays put while the search narrows the lists around it.
  const detail = current ? sections.find((section) => section.id === current.id)?.detail : undefined;

  const open = (id: SettingsTabId) => {
    const leaving = sections.find((section) => section.id === activeId);
    // A section holding unsaved edits keeps its page and asks before it is left.
    if (id !== activeId && leaving?.closeDetail && !leaving.closeDetail()) return;
    onSelectTab(id);
    setIndexOpen(false);
  };
  useBackNavigation(Boolean(onBackToApp), () => {
    const section = sections.find((candidate) => candidate.id === current?.id);
    if (section?.closeDetail && !indexOpen) section.closeDetail();
    else if (isNarrowSettingsViewport() && !indexOpen) setIndexOpen(true);
    else onBackToApp?.();
  }, 20);

  useEffect(() => {
    // The route names the section in view, so a shell that restores or gates on it sees the truth.
    if (state.activeTab !== activeId || !routeNamesSection()) onSelectTab(activeId);
  }, [activeId]);
  const phoneCapability = currentFrontendShell()?.connectionSettings;
  useEffect(() => {
    // Declared after the route effect: the phone shell refuses a request made
    // before the route names the section, and sends nothing on its own later.
    if (phoneSections.includes(activeId)) void phone?.execute({ action: "state" });
  }, [activeId, phoneCapability]);
  const enteredSection = useRef(activeId);
  useEffect(() => {
    if (enteredSection.current === activeId) return;
    enteredSection.current = activeId;
    if (projectedSections.includes(activeId)) onRefresh();
  }, [activeId, onRefresh]);
  const previousAgentStatuses = useRef(state.agentDetails);
  useEffect(() => {
    const previous = previousAgentStatuses.current;
    previousAgentStatuses.current = state.agentDetails;
    if (agentLeftLaunching(previous, state.agentDetails)) onRefresh();
  }, [onRefresh, state.agentDetails]);
  useEffect(() => {
    // A section change or a drill-in starts at its top, not at the previous scroll offset.
    pageRef.current?.scrollTo?.(0, 0);
  }, [current?.id, Boolean(detail)]);
  useEffect(() => {
    if (typeof window === "undefined") return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey) || event.altKey || event.key.toLowerCase() !== "f") return;
      event.preventDefault();
      setIndexOpen(true);
      searchRef.current?.focus();
      searchRef.current?.select();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  const sidebar = (
    <aside className="settings-nav">
      <header>
        {onBackToApp ? (
          <button aria-label="Close settings" className="settings-icon-button" onClick={onBackToApp} title="Back to tasks" type="button">
            <ArrowLeft aria-hidden="true" size={16} />
          </button>
        ) : null}
        <strong>Settings</strong>
      </header>
      <label className="settings-search">
        <Search aria-hidden="true" size={15} />
        <input
          aria-label="Search settings"
          onChange={(event) => setQuery(event.currentTarget.value)}
          onKeyDown={(event) => { if (event.key === "Escape" && query) setQuery(""); }}
          placeholder="Search"
          ref={searchRef}
          value={query}
        />
        {query ? <button aria-label="Clear search" onClick={() => setQuery("")} type="button"><X aria-hidden="true" size={13} /></button> : null}
      </label>
      <nav aria-label="Settings sections">
        {filtered.map((section) => (
          <button
            aria-current={current?.id === section.id ? "page" : undefined}
            id={`settings-section-${section.id}`}
            key={section.id}
            onClick={() => open(section.id)}
            type="button"
          >
            <section.icon aria-hidden="true" size={16} strokeWidth={1.6} />
            <span>{section.label}</span>
            {section.value ? <small>{section.value}</small> : null}
            <ChevronRight aria-hidden="true" className="settings-nav-chevron" size={16} />
          </button>
        ))}
        {!filtered.length ? (
          <p className="settings-nothing" role="status">
            Nothing found. <button onClick={() => setQuery("")} type="button">Clear search</button>
          </p>
        ) : null}
      </nav>
    </aside>
  );

  return (
    <AppSidebarFrame
      aria-label="Settings"
      className={`settings-view${indexOpen ? " settings-index-open" : ""}`}
      header={frameHeader}
      headerPlacement={frameHeaderPlacement}
      sidebar={sidebar}
    >
      <section aria-label={current?.label} className="settings-page" ref={pageRef}>
        {/* TODO: `state.error` has no origin, so every Settings failure lands here. Attribute
            errors to their operation in the Settings reducer so each shows at the control that caused it. */}
        {state.error && !detail ? (
          <div className="settings-page-error">
            <SettingsInlineError>{state.error}</SettingsInlineError>
            {onDismissError ? (
              <button aria-label="Dismiss error" className="settings-icon-button" onClick={onDismissError} type="button">
                <X aria-hidden="true" size={14} />
              </button>
            ) : null}
          </div>
        ) : null}
        {detail ?? (current ? (
          <>
            <header className="settings-page-header">
              <button aria-label="All settings" className="settings-icon-button settings-to-index" onClick={() => setIndexOpen(true)} type="button">
                <ArrowLeft aria-hidden="true" size={18} />
              </button>
              <h1>{mark(current.label, needle)}</h1>
              {current.info ? <InfoHint hit={infoMatches(current.info, needle)} info={current.info} /> : null}
              {current.action ? <span className="settings-header-end">{current.action}</span> : null}
            </header>
            {current.notice}
            {current.groups.map((group) => <SettingsGroupView group={group} key={group.id} needle={needle} />)}
          </>
        ) : null)}
        {current ? sections.find((section) => section.id === current.id)?.overlay : null}
      </section>
    </AppSidebarFrame>
  );
}

function isNarrowSettingsViewport() {
  if (typeof window === "undefined") return false;
  if (typeof window.matchMedia === "function") return window.matchMedia("(max-width: 760px)").matches;
  return window.innerWidth <= 760;
}

function routeNamesSection() {
  if (typeof window === "undefined" || !window.location) return true;
  return new URLSearchParams(window.location.search).has("tab");
}
