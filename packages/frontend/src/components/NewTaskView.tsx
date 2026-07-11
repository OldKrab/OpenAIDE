import { Check, FolderOpen } from "lucide-react";
import { useEffect, useRef, useState, type Dispatch } from "react";
import type { AppPreferencesRecord, TaskSummary } from "@openaide/app-shell-contracts";
import type { AppAction } from "../state/appReducer";
import { agentOptions, appServerAttachmentHandles, type AgentOption } from "../state/composerOptions";
import { projectIdForWorkspaceRoot, workspaceLabel } from "../state/projectIdentity";
import type { AppState } from "../state/store";
import { AgentIcon } from "./AgentIcon";
import { Composer } from "./Composer";
import { MenuButton, Popover, Selector } from "./ComposerPrimitives";
import type { TaskFileBrowserCallbacks, WorkspaceBrowserCallbacks } from "./appControllerCallbackTypes";
import { NewWorkspacePicker } from "./NewWorkspacePicker";
import { NewTaskStartingView } from "./NewTaskStartingView";
import { newTaskStatusLabel } from "./taskSurfaceHelpers";

type NewTaskContextMenu = "project" | "agent";

export function NewTaskView({
  dispatch,
  state,
  onSelectConfigOption,
  onCancelTask,
  onSubmitTask,
  resetOptionsRequestKey,
  agents,
  loadingProjects = false,
  submitShortcut,
  fileBrowser,
  focusRequestKey,
  workspaceBrowser,
}: {
  state: AppState;
  dispatch: Dispatch<AppAction>;
  fileBrowser?: TaskFileBrowserCallbacks;
  workspaceBrowser?: WorkspaceBrowserCallbacks;
  focusRequestKey?: number;
  loadingProjects?: boolean;
  onSelectConfigOption: (configId: string, value: string) => void;
  onCancelTask?: () => void;
  onSubmitTask: (draft: { prompt: string; context: AppState["newTask"]["context"] }) => void;
  resetOptionsRequestKey: () => void;
  agents?: AgentOption[];
  submitShortcut: AppPreferencesRecord["composer_submit_shortcut"];
}) {
  const [openContextMenu, setOpenContextMenu] = useState<NewTaskContextMenu | undefined>();
  const [workspacePath, setWorkspacePath] = useState(state.newTask.selection.workspaceRoot);
  const contextControlsRef = useRef<HTMLDivElement | null>(null);
  const agentChoices = agents?.length ? agents : agentOptions;
  const selectedAgent = agentChoices.find((agent) => agent.id === state.newTask.selection.agentId);
  const projectChoices = state.projects.length ? state.projects : projectsFromTasks(state.tasks);
  const selectedProject = projectChoices.find((project) => project.projectId === state.newTask.selection.projectId);
  const enteredWorkspacePath = workspacePath.trim();
  const preparedTaskId = state.snapshot && !state.snapshot.task.has_messages ? state.snapshot.task.task_id : undefined;
  const preparedTaskInput = preparedTaskId ? state.taskInputs[preparedTaskId] : undefined;
  const preparedConfigOptions = preparedTaskId ? state.snapshot?.agent_config : undefined;
  const composerConfigOptions = preparedConfigOptions?.options.length ? preparedConfigOptions : undefined;
  const composerConfigOptionsError = preparedTaskId ? undefined : state.newTask.configOptionsError;
  const composerAttachments = state.newTask.submitting
    ? state.newTask.pending?.context ?? preparedTaskInput?.pending?.context ?? []
    : preparedTaskInput?.context ?? state.newTask.context;
  const attachmentOnlySend = preparedTaskId !== undefined
    && state.snapshot?.send_capability.attachment_only === true;
  const showTextRequirementError = state.snapshot?.send_capability.state !== "loading";
  const externalPrompt = state.newTask.submitting
    ? state.newTask.pending?.prompt ?? preparedTaskInput?.pending?.prompt ?? ""
    : preparedTaskInput?.prompt ?? state.newTask.prompt;
  const [localPrompt, setLocalPrompt] = useState(externalPrompt);
  const lastExternalPromptRef = useRef(externalPrompt);
  useEffect(() => {
    if (externalPrompt === lastExternalPromptRef.current) return;
    lastExternalPromptRef.current = externalPrompt;
    setLocalPrompt(externalPrompt);
  }, [externalPrompt]);
  const composerPrompt = localPrompt;
  const needsProject = !state.newTask.selection.projectId;
  const openingNativeSession = state.newTask.nativeSessions.adoptingSessionId !== undefined;
  const projectSelectorLabel = selectedProject?.label
    ?? (state.newTask.selection.projectId ? state.newTask.selection.workspaceLabel : loadingProjects ? "Loading" : "Choose workspace");
  const needsWorkspace = state.workspaceRootsLoaded && state.projects.length === 0 && state.newTask.selection.workspaceRoot.trim().length === 0;
  const waitStatus = newTaskStatusLabel({
    agentLabel: state.newTask.selection.agentLabel,
    configOptionsError: composerConfigOptionsError,
    configOptionsLoading: state.newTask.configOptionsLoading,
    configOptionsReady: composerConfigOptions !== undefined,
    needsWorkspace,
    openingNativeSession,
    submitting: state.newTask.submitting,
  });
  const canSend =
    !needsProject &&
    !needsWorkspace &&
    !loadingProjects &&
    !state.newTask.submitting &&
    !composerConfigOptionsError &&
    (composerAttachments.length === 0 || appServerAttachmentHandles(composerAttachments) !== undefined);
  const composerFocusKey = `${focusRequestKey ?? 0}:${canSend && composerPrompt.trim().length > 0 ? "ready" : "waiting"}`;
  const composerFileBrowser = needsProject || needsWorkspace || loadingProjects ? undefined : fileBrowser;

  const submit = (prompt: string) => {
    if (!canSend) return;
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
    resetOptionsRequestKey();
    dispatch({
      type: "newTask:workspace",
      workspace: {
        path: trimmedPath,
        label,
        projectId: projectIdForWorkspaceRoot(trimmedPath),
      },
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
      configLocked={state.newTask.configOptionsLoading}
      configOptions={composerConfigOptions}
      commandCatalog={preparedTaskId ? state.snapshot?.agent_commands : undefined}
      disabled={state.newTask.submitting}
      error={undefined}
      fileBrowser={composerFileBrowser}
      focusRequestKey={composerFocusKey}
      onCancel={state.newTask.submitting ? onCancelTask : undefined}
      onChange={(prompt) => {
        setLocalPrompt(prompt);
        dispatch(preparedTaskId
          ? { type: "taskInput:prompt", taskId: preparedTaskId, prompt }
          : { type: "prompt", prompt });
      }}
      onUnsupportedImageAttachment={(message) =>
        dispatch({
          type: "submit:error",
          message: message ?? "Images can be attached after the Task is open.",
        })
      }
      onRemoveAttachment={(attachmentId) =>
        preparedTaskId
          ? dispatch({ type: "taskInput:attachment:remove", taskId: preparedTaskId, attachmentId })
          : dispatch({ type: "newTask:attachment:remove", attachmentId })}
      onSelectAgent={(agentId) => {
        resetOptionsRequestKey();
        dispatch({
          type: "newTask:agent",
          agentId,
          agentLabel: agentChoices.find((agent) => agent.id === agentId)?.label,
        });
      }}
      onSelectConfigOption={onSelectConfigOption}
      onSelectIsolation={(isolation) => dispatch({ type: "newTask:isolation", isolation })}
      onSubmit={submit}
      placeholder={state.newTask.submitting ? "" : "Describe the task."}
      prompt={composerPrompt}
      selection={state.newTask.selection}
      agents={agentChoices}
      submitShortcut={submitShortcut}
      submitDisabled={!canSend}
      submitPending={state.newTask.submitting}
      submitRequiresText={!attachmentOnlySend}
      showTextRequirementError={showTextRequirementError}
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
          <div className={`new-task-context-anchor new-task-context-anchor-project ${openContextMenu === "project" ? "context-menu-open" : ""}`}>
            <Selector
              disabled={loadingProjects || state.newTask.submitting}
              icon={<FolderOpen size={12} />}
              label={projectSelectorLabel}
              locked={false}
              menuOpen={openContextMenu === "project"}
              onClick={() => toggleContextMenu("project")}
            />
            {openContextMenu === "project" ? (
              <Popover className="new-task-context-menu" label="Project">
                {projectChoices.length ? <div className="new-task-context-menu-heading" role="none">Projects</div> : null}
                {projectChoices.map((project) => (
                  <MenuButton
                    active={state.newTask.selection.projectId === project.projectId}
                    icon={<FolderOpen size={13} />}
                    key={project.projectId}
                    label={project.label}
                    onClick={() => selectContextAndClose(() => {
                      resetOptionsRequestKey();
                      dispatch({ type: "newTask:project", project });
                    })}
                  />
                ))}
                {workspaceBrowser ? (
                  <NewWorkspacePicker
                    browser={workspaceBrowser}
                    onSelect={(workspace) => selectWorkspacePath(workspace.path, workspace.label)}
                  />
                ) : null}
                <div className="new-workspace-entry" role="none">
                  <label htmlFor="new-task-workspace-root">Open workspace path</label>
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
                      placeholder="/path/to/project"
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
          </div>
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
                        resetOptionsRequestKey();
                        dispatch({
                          type: "newTask:agent",
                          agentId: agent.id,
                          agentLabel: agent.label,
                        });
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
        {needsProject ? <p className="inline-hint">{loadingProjects ? "Loading projects." : "Choose or enter a workspace to start a task."}</p> : null}
        {needsWorkspace ? <p className="inline-hint">Enter a workspace path to start a task.</p> : null}
        {composerConfigOptionsError ? <p className="inline-error">{composerConfigOptionsError}</p> : null}
        {state.newTask.error ? <p className="inline-error">{state.newTask.error}</p> : null}
      </div>
    </section>
  );
}

function projectsFromTasks(tasks: TaskSummary[]) {
  const projects = new Map<string, { projectId: string; label: string }>();
  for (const task of tasks) {
    if (!task.project_id || projects.has(task.project_id)) continue;
    projects.set(task.project_id, {
      projectId: task.project_id,
      label: task.project_label || task.workspace_root || "Project",
    });
  }
  return [...projects.values()];
}
