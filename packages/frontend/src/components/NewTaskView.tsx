import { Check, FolderOpen, FolderRoot, GitBranch } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { AppPreferencesRecord } from "@openaide/app-shell-contracts";
import {
  agentOptions,
  appServerComposerImages,
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
import { TaskWorkspacePicker } from "./TaskWorkspacePicker";
import { AgentRecoveryPanel, agentRecoveryKind, type AgentRecoveryActions } from "./AgentRecovery";

type NewTaskContextMenu = "project" | "workspace" | "agent";
export type ProjectContextMode = "fixed" | "selectable";

export type NewTaskViewState = {
  newTask: NewTaskState;
  preparedTaskInput?: TaskComposerInput;
  projects: AppState["projects"];
  tasks: AppState["tasks"];
  worktreeRepositories: AppState["worktreeRepositories"];
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
  selectWorktree: (worktree: { worktreeId?: string; label: string; path: string }) => void;
  refreshWorktrees: (project: ProjectOption) => Promise<void>;
  createWorktree: (project: ProjectOption, draft: { name: string; base: import("@openaide/app-server-client").WorktreeBaseSelection; branch?: string }, onProgress?: (operation: import("@openaide/app-server-client").WorktreeOperationSnapshot) => void) => Promise<import("@openaide/app-server-client").WorktreeSummary>;
  recreateWorktree: (project: ProjectOption, worktreeId: string, draft: { base: import("@openaide/app-server-client").WorktreeBaseSelection; branch?: string }, onProgress?: (operation: import("@openaide/app-server-client").WorktreeOperationSnapshot) => void) => Promise<import("@openaide/app-server-client").WorktreeSummary>;
  removeWorktree: (repositoryId: string, worktreeId: string) => Promise<void>;
  removalPreflight: (repositoryId: string, worktreeId: string) => Promise<import("@openaide/app-server-client").WorktreeRemovalPreflight>;
  renameWorktree: (repositoryId: string, worktreeId: string, name: string) => Promise<void>;
  openFolder?: (repositoryId: string, worktreeId: string) => void;
  loadProjectTasks?: (projectId: string) => Promise<import("@openaide/app-shell-contracts").TaskSummary[]>;
  openTask: (taskId: string) => void;
};

export function NewTaskView({
  intents,
  state,
  onSelectConfigOption,
  onCancelTask,
  onRemoveAttachment,
  onSubmitTask,
  agents,
  agentRecoveryActions,
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
  agentRecoveryActions?: AgentRecoveryActions;
  submitShortcut: AppPreferencesRecord["composer_submit_shortcut"];
}) {
  const [openContextMenu, setOpenContextMenu] = useState<NewTaskContextMenu | undefined>();
  const [workspacePath, setWorkspacePath] = useState(state.newTask.selection.workspaceRoot);
  const contextControlsRef = useRef<HTMLDivElement | null>(null);
  const agentChoices = agents?.length ? agents : agentOptions;
  const selectedAgent = agentChoices.find((agent) => agent.id === state.newTask.selection.agentId);
  const recoveryKind = agentRecoveryKind(selectedAgent, state.snapshot?.preparation);
  const projectChoices = state.projects;
  const selectedProject = projectChoices.find((project) => project.projectId === state.newTask.selection.projectId);
  const selectedRepository = selectedProject?.worktreeRepositoryId
    ? state.worktreeRepositories[selectedProject.worktreeRepositoryId]
    : undefined;
  const selectedWorktree = selectedRepository?.worktrees.find((worktree) => worktree.worktreeId === state.newTask.selection.worktreeId);
  const worktreeSelected = Boolean(state.newTask.selection.worktreeId);
  const worktreeLoading = worktreeSelected && !selectedRepository;
  const worktreeUnavailable = Boolean(worktreeSelected && selectedRepository
    && (!selectedWorktree || selectedWorktree.availability === "unavailable"));
  const taskWorkspaceLabel = worktreeSelected
    ? selectedWorktree?.name ?? state.newTask.selection.workspaceLabel ?? "Workspace unavailable"
    : "Project root";
  const enteredWorkspacePath = workspacePath.trim();
  const preparedTaskId = state.snapshot && !state.snapshot.task.has_messages ? state.snapshot.task.task_id : undefined;
  const preparedConfigOptions = preparedTaskId ? state.snapshot?.agent_config : undefined;
  const currentConfigOptions = preparedTaskId ? preparedConfigOptions : state.newTask.configOptions;
  const composerConfigOptions = currentConfigOptions && (
    currentConfigOptions.options.length > 0 || currentConfigOptions.status === "empty"
  ) ? currentConfigOptions : undefined;
  const composerConfigOptionsError = currentConfigOptions?.status === "failed"
    ? currentConfigOptions.error
    : preparedTaskId ? undefined : state.newTask.configOptionsError;
  const composerAttachments = state.newTask.submitting
    ? state.newTask.pending?.context ?? []
    : state.newTask.context;
  const externalPrompt = state.newTask.submitting
    ? state.newTask.pending?.prompt ?? ""
    : state.newTask.prompt;
  const composerPrompt = externalPrompt;
  const needsProject = !state.newTask.selection.projectId;
  const fixedProjectContext = projectContextMode === "fixed";
  const openingNativeSession = state.newTask.nativeSessions.adoptingSessionId !== undefined;
  const projectSelectorLabel = selectedProject?.label
    ?? (state.newTask.selection.projectId ? state.newTask.selection.workspaceLabel : loadingProjects ? "Loading" : "Choose workspace");
  const needsWorkspace = state.workspaceRootsLoaded && state.projects.length === 0 && state.newTask.selection.workspaceRoot.trim().length === 0;
  const projectUnavailable = selectedProject?.available === false;
  const waitStatus = newTaskStatusLabel({
    agentLabel: state.newTask.selection.agentLabel,
    configOptionsError: composerConfigOptionsError,
    configOptionsLoading: state.newTask.configOptionsLoading,
    configOptionsReady: configOptionsSettled(currentConfigOptions),
    needsWorkspace,
    openingNativeSession,
    submitting: state.newTask.submitting,
  });
  // Keep local drafting available while capability discovery is pending; block only
  // when the selected prepared Task explicitly reports that Images are unsupported.
  const imageAttachmentsAllowed = state.snapshot?.input_capabilities?.image !== false;
  const attachmentsReady = (composerAttachments.length === 0
    || appServerComposerImages(composerAttachments) !== undefined)
    && (composerAttachments.length === 0 || imageAttachmentsAllowed);
  const availability = composerAvailability({
    allowEditingWhileSendBlocked: false,
    attachmentsReady,
    attachmentsBlockedMessage: composerAttachments.length > 0 && !imageAttachmentsAllowed
      ? "This Agent does not accept images."
      : "Attached context is not ready to send.",
    connectionStatus: loadingProjects ? "connecting" : "ready",
    contextPlaceholder: projectUnavailable
      ? "Project folder unavailable."
      : worktreeUnavailable
        ? "Workspace unavailable. Choose another workspace."
        : worktreeLoading ? "Loading workspace." : waitStatus ?? "Preparing task.",
    contextReady: !needsProject && !needsWorkspace && !projectUnavailable && !worktreeUnavailable
      && !worktreeLoading && !loadingProjects && !composerConfigOptionsError,
    readyPlaceholder: "Describe the task.",
    sendCapability: preparedTaskId ? state.snapshot?.send_capability : undefined,
    submitPendingLabel: "Task starting",
    submitting: state.newTask.submitting,
  });
  const canSubmit = composerCanSubmit(availability, composerPrompt, composerAttachments.length);
  const composerFocusKey = `${focusRequestKey ?? 0}:${canSubmit ? "ready" : "waiting"}`;
  const composerFileBrowser = needsProject || needsWorkspace || projectUnavailable
    || worktreeUnavailable || worktreeLoading || loadingProjects ? undefined : fileBrowser;

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
      imageAttachmentsAllowed={imageAttachmentsAllowed}
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
          {selectedProject?.worktreeRepositoryId ? <div className={`new-task-context-anchor new-task-context-anchor-workspace ${openContextMenu === "workspace" ? "context-menu-open" : ""}`}>
            <Selector
              disabled={state.newTask.submitting}
              icon={state.newTask.selection.worktreeId ? <GitBranch size={12} /> : <FolderRoot size={12} />}
              label={taskWorkspaceLabel}
              locked={false}
              menuOpen={openContextMenu === "workspace"}
              onClick={() => toggleContextMenu("workspace")}
            />
            {openContextMenu === "workspace" ? (
              <TaskWorkspacePicker
                intents={intents}
                onClose={() => setOpenContextMenu(undefined)}
                project={selectedProject}
                repository={selectedRepository}
                selectedWorktreeId={state.newTask.selection.worktreeId}
                tasks={state.tasks}
              />
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
        {recoveryKind && selectedAgent && agentRecoveryActions ? (
          <AgentRecoveryPanel
            actions={agentRecoveryActions}
            agent={selectedAgent}
            kind={recoveryKind}
            returnToNewTask
          />
        ) : composer}
        {!recoveryKind && waitStatus && !state.newTask.submitting ? (
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
        {projectUnavailable ? <p className="inline-hint error">This Project folder is unavailable. Restore it before starting a task.</p> : null}
        {worktreeUnavailable ? <p className="inline-hint error">Workspace unavailable. Choose another workspace to keep this draft.</p> : null}
        {composerConfigOptionsError ? <p className="inline-error">{composerConfigOptionsError}</p> : null}
        {state.newTask.error ? <p className="inline-error">{state.newTask.error}</p> : null}
      </div>
    </section>
  );
}
