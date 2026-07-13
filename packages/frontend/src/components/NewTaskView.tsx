import { Check, FolderOpen } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { AppPreferencesRecord } from "@openaide/app-shell-contracts";
import {
  agentOptions,
  appServerAttachmentHandles,
  type AgentOption,
  type ComposerSelection,
  type ProjectOption,
} from "../state/composerOptions";
import { projectIdForWorkspaceRoot, workspaceLabel } from "../state/projectIdentity";
import { configOptionsMutable, configOptionsSettled } from "../state/configOptionState";
import type { AppState, NewTaskState, TaskComposerInput } from "../state/store";
import { AgentIcon } from "./AgentIcon";
import { Composer } from "./Composer";
import { composerAvailability, composerCanSubmit } from "./composerAvailability";
import { MenuButton, Popover, Selector } from "./ComposerPrimitives";
import type { TaskFileBrowserCallbacks, WorkspaceBrowserCallbacks } from "./appControllerCallbackTypes";
import { NewWorkspacePicker } from "./NewWorkspacePicker";
import { NewTaskStartingView } from "./NewTaskStartingView";
import { newTaskStatusLabel } from "./taskSurfaceHelpers";

type NewTaskContextMenu = "project" | "agent";
export type ProjectContextMode = "fixed" | "selectable";

export type NewTaskViewState = {
  newTask: NewTaskState;
  preparedTaskInput?: TaskComposerInput;
  projects: AppState["projects"];
  snapshot?: AppState["snapshot"];
  workspaceRootsLoaded: boolean;
};

export type NewTaskViewIntents = {
  changePrompt: (prompt: string) => void;
  reportAttachmentError: (message?: string) => void;
  selectAgent: (agentId: string, agentLabel?: string) => void;
  selectIsolation: (isolation: ComposerSelection["isolation"]) => void;
  selectProject: (project: ProjectOption) => void;
  selectWorkspace: (workspace: { path: string; label: string; projectId: string }) => void;
};

export function NewTaskView({
  intents,
  state,
  onSelectConfigOption,
  onCancelTask,
  onRemoveAttachment,
  onSubmitTask,
  agents,
  loadingProjects = false,
  submitShortcut,
  fileBrowser,
  focusRequestKey,
  workspaceBrowser,
  projectContextMode = "selectable",
}: {
  state: NewTaskViewState;
  intents: NewTaskViewIntents;
  fileBrowser?: TaskFileBrowserCallbacks;
  workspaceBrowser?: WorkspaceBrowserCallbacks;
  projectContextMode?: ProjectContextMode;
  focusRequestKey?: number;
  loadingProjects?: boolean;
  onSelectConfigOption: (configId: string, value: string) => void;
  onCancelTask?: () => void;
  onRemoveAttachment: (attachmentId: string) => void;
  onSubmitTask: (draft: { prompt: string; context: AppState["newTask"]["context"] }) => void;
  agents?: AgentOption[];
  submitShortcut: AppPreferencesRecord["composer_submit_shortcut"];
}) {
  const [openContextMenu, setOpenContextMenu] = useState<NewTaskContextMenu | undefined>();
  const [workspacePath, setWorkspacePath] = useState(state.newTask.selection.workspaceRoot);
  const contextControlsRef = useRef<HTMLDivElement | null>(null);
  const agentChoices = agents?.length ? agents : agentOptions;
  const selectedAgent = agentChoices.find((agent) => agent.id === state.newTask.selection.agentId);
  const projectChoices = state.projects;
  const selectedProject = projectChoices.find((project) => project.projectId === state.newTask.selection.projectId);
  const enteredWorkspacePath = workspacePath.trim();
  const preparedTaskId = state.snapshot && !state.snapshot.task.has_messages ? state.snapshot.task.task_id : undefined;
  const preparedTaskInput = preparedTaskId ? state.preparedTaskInput : undefined;
  const preparedConfigOptions = preparedTaskId ? state.snapshot?.agent_config : undefined;
  const currentConfigOptions = preparedTaskId ? preparedConfigOptions : state.newTask.configOptions;
  const composerConfigOptions = currentConfigOptions && (
    currentConfigOptions.options.length > 0 || currentConfigOptions.status === "empty"
  ) ? currentConfigOptions : undefined;
  const composerConfigOptionsError = currentConfigOptions?.status === "failed"
    ? currentConfigOptions.error
    : preparedTaskId ? undefined : state.newTask.configOptionsError;
  const composerAttachments = state.newTask.submitting
    ? state.newTask.pending?.context ?? preparedTaskInput?.pending?.context ?? []
    : preparedTaskInput?.context ?? state.newTask.context;
  const externalPrompt = state.newTask.submitting
    ? state.newTask.pending?.prompt ?? preparedTaskInput?.pending?.prompt ?? ""
    : preparedTaskInput?.prompt ?? state.newTask.prompt;
  const composerPrompt = externalPrompt;
  const needsProject = !state.newTask.selection.projectId;
  const fixedProjectContext = projectContextMode === "fixed";
  const openingNativeSession = state.newTask.nativeSessions.adoptingSessionId !== undefined;
  const projectSelectorLabel = selectedProject?.label
    ?? (state.newTask.selection.projectId ? state.newTask.selection.workspaceLabel : loadingProjects ? "Loading" : "Choose workspace");
  const needsWorkspace = state.workspaceRootsLoaded && state.projects.length === 0 && state.newTask.selection.workspaceRoot.trim().length === 0;
  const waitStatus = newTaskStatusLabel({
    agentLabel: state.newTask.selection.agentLabel,
    configOptionsError: composerConfigOptionsError,
    configOptionsLoading: state.newTask.configOptionsLoading,
    configOptionsReady: configOptionsSettled(currentConfigOptions),
    needsWorkspace,
    openingNativeSession,
    submitting: state.newTask.submitting,
  });
  const availability = composerAvailability({
    allowEditingWhileSendBlocked: false,
    attachmentsReady: composerAttachments.length === 0
      || appServerAttachmentHandles(composerAttachments) !== undefined,
    connectionStatus: loadingProjects ? "connecting" : "ready",
    contextPlaceholder: waitStatus ?? "Preparing task.",
    contextReady: !needsProject && !needsWorkspace && !loadingProjects && !composerConfigOptionsError,
    readyPlaceholder: "Describe the task.",
    sendCapability: preparedTaskId ? state.snapshot?.send_capability : undefined,
    submitPendingLabel: "Task starting",
    submitting: state.newTask.submitting,
  });
  const canSubmit = composerCanSubmit(availability, composerPrompt, composerAttachments.length);
  const composerFocusKey = `${focusRequestKey ?? 0}:${canSubmit ? "ready" : "waiting"}`;
  const composerFileBrowser = needsProject || needsWorkspace || loadingProjects ? undefined : fileBrowser;

  const submit = (prompt: string) => {
    if (!canSubmit) return;
    onSubmitTask({ prompt, context: composerAttachments });
  };
  const toggleContextMenu = (menu: NewTaskContextMenu) => {
    setOpenContextMenu((current) => current === menu ? undefined : menu);
  };
  const selectContextAndClose = (select: () => void) => {
    select();
    setOpenContextMenu(undefined);
  };
  const selectWorkspacePath = (path: string, label = workspaceLabel(path)) => {
    const trimmedPath = path.trim();
    if (!trimmedPath) return;
    intents.selectWorkspace({
      path: trimmedPath,
      label,
      projectId: projectIdForWorkspaceRoot(trimmedPath),
    });
    setOpenContextMenu(undefined);
  };
  const useWorkspacePath = () => selectWorkspacePath(enteredWorkspacePath);
  useEffect(() => {
    if (!openContextMenu || typeof document === "undefined") return undefined;
    const onPointerDown = (event: PointerEvent) => {
      if (contextControlsRef.current?.contains(event.target as Node)) return;
      setOpenContextMenu(undefined);
    };
    document.addEventListener("pointerdown", onPointerDown);
    return () => document.removeEventListener("pointerdown", onPointerDown);
  }, [openContextMenu]);

  const composer = (
    <Composer
      attachments={composerAttachments}
      autoFocus
      availability={availability}
      configLocked={state.newTask.configOptionsLoading || !configOptionsMutable(currentConfigOptions)}
      configOptions={composerConfigOptions}
      commandCatalog={preparedTaskId ? state.snapshot?.agent_commands : undefined}
      error={undefined}
      fileBrowser={composerFileBrowser}
      focusRequestKey={composerFocusKey}
      onCancel={state.newTask.submitting ? onCancelTask : undefined}
      onChange={intents.changePrompt}
      onUnsupportedImageAttachment={intents.reportAttachmentError}
      onRemoveAttachment={onRemoveAttachment}
      onSelectAgent={(agentId) => {
        intents.selectAgent(agentId, agentChoices.find((agent) => agent.id === agentId)?.label);
      }}
      onSelectConfigOption={onSelectConfigOption}
      onSelectIsolation={intents.selectIsolation}
      onSubmit={submit}
      prompt={composerPrompt}
      selection={state.newTask.selection}
      agents={agentChoices}
      submitShortcut={submitShortcut}
      showAgentSelector={false}
      showIsolationSelector={false}
    />
  );

  if (state.newTask.submitting && openingNativeSession) {
    return (
      <NewTaskStartingView
        agentId={state.newTask.selection.agentId}
        agentName={state.newTask.selection.agentLabel}
        composer={composer}
        openingNativeSession={openingNativeSession}
        workspaceRoot={state.newTask.selection.workspaceRoot}
      />
    );
  }

  return (
    <section
      className="task-surface new-task-surface"
      aria-label="New task"
      onKeyDown={(event) => {
        if (event.key === "Escape") setOpenContextMenu(undefined);
      }}
      onPointerDownCapture={(event) => {
        if (!openContextMenu) return;
        if (contextControlsRef.current?.contains(event.target as Node)) return;
        setOpenContextMenu(undefined);
      }}
    >
      <div className="new-task-center">
        <h1>What are we working on?</h1>
        <div className="new-task-context-controls" aria-label="Task start context" ref={contextControlsRef}>
          {!fixedProjectContext ? <div className={`new-task-context-anchor new-task-context-anchor-project ${openContextMenu === "project" ? "context-menu-open" : ""}`}>
            <Selector
              disabled={loadingProjects || state.newTask.submitting}
              icon={<FolderOpen size={12} />}
              label={projectSelectorLabel}
              locked={false}
              menuOpen={openContextMenu === "project"}
              onClick={() => toggleContextMenu("project")}
            />
            {openContextMenu === "project" ? (
              <Popover className="new-task-context-menu" label="Workspace">
                {projectChoices.length ? <div className="new-task-context-menu-heading" role="none">Workspaces</div> : null}
                {projectChoices.map((project) => (
                  <MenuButton
                    active={state.newTask.selection.projectId === project.projectId}
                    icon={<FolderOpen size={13} />}
                    key={project.projectId}
                    label={project.label}
                    onClick={() => selectContextAndClose(() => {
                      intents.selectProject(project);
                    })}
                  />
                ))}
                {workspaceBrowser ? (
                  <NewWorkspacePicker
                    browser={workspaceBrowser}
                    key={workspaceBrowser.ownerKey}
                    onSelect={(workspace) => selectWorkspacePath(workspace.path, workspace.label)}
                  />
                ) : null}
                <div className="new-workspace-entry" role="none">
                  <label htmlFor="new-task-workspace-root">Open folder path</label>
                  <div className="new-workspace-entry-row">
                    <input
                      id="new-task-workspace-root"
                      onChange={(event) => setWorkspacePath(event.target.value)}
                      onKeyDown={(event) => {
                        if (event.key === "Enter") {
                          event.preventDefault();
                          useWorkspacePath();
                        }
                      }}
                      placeholder="/path/to/workspace"
                      type="text"
                      value={workspacePath}
                    />
                    <button
                      aria-label="Use workspace path"
                      disabled={!enteredWorkspacePath}
                      onClick={useWorkspacePath}
                      type="button"
                    >
                      <Check size={14} />
                      <span>Open</span>
                    </button>
                  </div>
                </div>
              </Popover>
            ) : null}
          </div> : null}
          <div className={`new-task-context-anchor new-task-context-anchor-agent ${openContextMenu === "agent" ? "context-menu-open" : ""}`}>
            <Selector
              disabled={state.newTask.submitting}
              icon={(
                <AgentIcon
                  agentId={selectedAgent?.id ?? state.newTask.selection.agentId}
                  agentName={selectedAgent?.label ?? state.newTask.selection.agentLabel}
                  icon={selectedAgent?.icon}
                  size={12}
                />
              )}
              label={state.newTask.selection.agentLabel}
              locked={false}
              menuOpen={openContextMenu === "agent"}
              onClick={() => toggleContextMenu("agent")}
            />
            {openContextMenu === "agent" ? (
              <Popover className="new-task-context-menu" label="Agent">
                {agentChoices.filter((agent) => agent.enabled !== false).map((agent) => (
                  <MenuButton
                    active={state.newTask.selection.agentId === agent.id}
                    description={agent.description}
                    icon={<AgentIcon icon={agent.icon} size={13} />}
                    key={agent.id}
                    label={agent.label}
                    onClick={() =>
                      selectContextAndClose(() => {
                        intents.selectAgent(agent.id, agent.label);
                      })
                    }
                  />
                ))}
              </Popover>
            ) : null}
          </div>
        </div>
        {composer}
        {waitStatus && !state.newTask.submitting ? (
          <div className="inline-status" role="status" aria-live="polite">
            <span className="working-status-dots" aria-hidden="true">
              <span />
              <span />
              <span />
            </span>
            <span>{waitStatus}</span>
          </div>
        ) : null}
        {fixedProjectContext && !state.workspaceRootsLoaded ? <p className="inline-hint">Loading workspace.</p> : null}
        {fixedProjectContext && state.workspaceRootsLoaded && (needsProject || needsWorkspace) ? (
          <p className="inline-hint">Open a folder in VS Code to start a task.</p>
        ) : null}
        {!fixedProjectContext && needsProject ? <p className="inline-hint">{loadingProjects ? "Loading workspaces." : "Choose or enter a workspace to start a task."}</p> : null}
        {!fixedProjectContext && needsWorkspace ? <p className="inline-hint">Enter a workspace path to start a task.</p> : null}
        {composerConfigOptionsError ? <p className="inline-error">{composerConfigOptionsError}</p> : null}
        {state.newTask.error ? <p className="inline-error">{state.newTask.error}</p> : null}
      </div>
    </section>
  );
}
