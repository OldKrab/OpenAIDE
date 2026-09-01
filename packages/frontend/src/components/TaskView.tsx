import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { CircleAlert, ListTodo } from "lucide-react";
import type {
  AppPreferencesRecord,
  ConfigOptionCurrentValue,
  ConfigOptionsCatalog,
  ElicitationResponse,
  TaskSnapshot,
  TaskSummary,
} from "@openaide/app-shell-contracts";
import type { AppServerSession, BackendConnection, ToolImagePreview } from "@openaide/app-server-client";
import { renderedChat } from "../state/chatPaging";
import type {
  AppState,
  TaskChatScrollState,
  TaskComposerInput,
  TaskLiveTextPresentation,
  TaskOpenError,
} from "../state/store";
import { Composer } from "./Composer";
import { composerAvailability, composerCanSubmit } from "./composerAvailability";
import { TaskHeader } from "./TaskHeader";
import type { DesktopWindowCapability } from "../services/frontendShell";
import { scrollTopAfterPrependedContent } from "./TaskViewModel";
import { taskWorkingStatusLabel, workspaceLabel } from "./taskSurfaceHelpers";
import type { TaskFileBrowserCallbacks } from "./appControllerCallbackTypes";
import {
  permissionResponseForMessage,
  questionResponseForMessage,
} from "./taskChatPresentation";
import { useTaskChatScroll } from "./useTaskChatScroll";
import { appServerAttachmentHandles, appServerComposerImages } from "../state/composerOptions";
import { configOptionsMutable } from "../state/configOptionState";
import type { BackendConnectionState } from "./appControllerBackendLifecycle";
import type { AgentOption } from "../state/composerOptions";
import { AgentRecoveryPanel, taskAgentRecovery, type AgentRecoveryActions } from "./AgentRecovery";
import { AgentFileOpenContext } from "./agentFileOpen";
import { ComposerWithContextUsage } from "./ContextUsageIndicator";
import { AgentPlanView, resetAgentPlanDisclosure } from "./AgentPlan";
import { FileViewerPanel, TaskPanelToggle } from "./FileViewerPanel";
import { TaskMessageQueueView } from "./TaskMessageQueue";
import { buildTaskChatTimelineRows, TaskChatTimeline } from "./TaskChatTimeline";
import { installTaskQueueOverlayClearance } from "./taskQueueOverlayClearance";
import { TaskSessionReloadNotice } from "./TaskSessionReloadNotice";
import { currentFrontendShell } from "../services/frontendShell";
import { useTaskFileViewer } from "./useTaskFileViewer";
import { useSubagentSessions } from "./useSubagentSessions";
import { SubagentNavigator } from "./SubagentNavigator";
import { mapProtocolChatItem } from "../state/appServerProtocolChatMapping";

export {
  scrollTopAfterPrependedContent,
} from "./TaskViewModel";
export {
  permissionResponseForMessage,
  questionResponseForMessage,
} from "./taskChatPresentation";
export { chatRowKey, formatElapsedDuration } from "./TaskChatTimeline";


export type TaskViewIntents = {
  changePrompt: (prompt: string) => void;
  refreshWorkspace: () => Promise<void>;
  recordScroll: (scrollState: TaskChatScrollState) => void;
  reportAttachmentError: (message?: string) => void;
};

export function TaskLoadingView({
  error,
  errorKind,
  label = "Opening task",
  onRetry,
}: {
  error?: string;
  errorKind?: TaskOpenError["kind"];
  label?: string;
  onRetry?: () => void;
}) {
  if (error) {
    if (errorKind === "conflict") {
      return (
        <section className="task-surface task-loading" aria-label="Session open elsewhere">
          <p>Session open elsewhere.</p>
          <small className="inline-error" role="alert">{error}</small>
        </section>
      );
    }
    if (errorKind === "notFound") {
      return (
        <section className="task-surface task-loading" aria-label="Task not found">
          <p>Task not found.</p>
          <small className="inline-error" role="alert">
            This Task does not exist or is no longer available.
          </small>
        </section>
      );
    }
    return (
      <section className="task-surface task-loading" aria-label="Unable to open task">
        <p>Unable to open task.</p>
        <small className="inline-error" role="alert">
          {error}
        </small>
        {onRetry ? <button type="button" onClick={onRetry}>Retry</button> : null}
      </section>
    );
  }
  return (
    <section className="task-surface task-loading" aria-label={label}>
      <div className="task-loading-status" role="status" aria-live="polite">
        <span className="working-status-dots" aria-hidden="true">
          <span />
          <span />
          <span />
        </span>
        <span>{label}</span>
      </div>
    </section>
  );
}

export function TaskView({
  activeTask,
  agents,
  agentRecoveryActions,
  archived = false,
  backendConnectionState,
  subagentConnection,
  backendReady,
  taskMutationReady = backendReady,
  chatPageState,
  desktopWindow,
  intents,
  onCancel,
  onClosePlan,
  onAddToQueue,
  fileBrowser,
  fileViewer: fileViewerConnection,
  onLoadChatPage,
  onLoadComposerHistory,
  onLoadToolImagePreview,
  onManageWorktrees,
  onOpenProjectSettings,
  onSubscribeToolDetail,
  onPermissionRespond,
  onQuestionRespond,
  onReconnectProject,
  onRetryConnection,
  onRetryConfigOptions,
  onRevealAttachment,
  onRemoveAttachment,
  onRemoveQueueMessage,
  onReloadNativeSession,
  onTakeQueueMessage,
  onMoveQueueMessage,
  onPlanDrawerOpenChange,
  onSendQueueMessageNow,
  onRestoreTask,
  onSendPrompt,
  onPermissionPolicyChange,
  onSelectConfigOption,
  permissionResponses,
  planDrawerOpen = false,
  liveTextPresentation,
  questionResponses = {},
  startupConfigOptions,
  snapshot,
  savedScrollState,
  taskInput,
  toolDetails,
  submitShortcut,
  showWorkspaceContext = true,
}: {
  activeTask?: TaskSummary;
  agents?: AgentOption[];
  agentRecoveryActions?: AgentRecoveryActions;
  archived?: boolean;
  backendConnectionState?: BackendConnectionState;
  subagentConnection?: Pick<AppServerSession, "request" | "subscribeState">;
  backendReady: boolean;
  taskMutationReady?: boolean;
  chatPageState: AppState["chatPages"][string] | undefined;
  desktopWindow?: DesktopWindowCapability;
  intents: TaskViewIntents;
  onCancel: () => void;
  onClosePlan?: () => Promise<void>;
  onAddToQueue?: () => void;
  fileBrowser?: TaskFileBrowserCallbacks;
  fileViewer?: Pick<BackendConnection, "request">;
  onLoadChatPage: (beforeCursor: string) => number | undefined;
  onLoadComposerHistory?: () => Promise<string[]>;
  onLoadToolImagePreview?: (artifactId: string) => Promise<ToolImagePreview | undefined>;
  onManageWorktrees?: (projectId: string) => void;
  onOpenProjectSettings?: () => void;
  onSubscribeToolDetail: (artifactId: string) => () => void;
  onPermissionRespond: (
    requestId: string,
    optionId: string,
  ) => void;
  onQuestionRespond?: (requestId: string, response: ElicitationResponse) => void;
  onReconnectProject?: (projectId: string) => void;
  onRetryConnection?: () => void;
  onRetryConfigOptions?: () => void;
  onRevealAttachment: (attachmentId: string) => Promise<void> | void;
  onRemoveAttachment: (attachmentId: string) => void;
  onRemoveQueueMessage?: (queuedMessageId: string) => void;
  onReloadNativeSession?: () => Promise<void>;
  onTakeQueueMessage?: (queuedMessageId: string) => void;
  onMoveQueueMessage?: (queuedMessageId: string, targetIndex: number) => void | Promise<void>;
  onPlanDrawerOpenChange?: (open: boolean) => void;
  onSendQueueMessageNow?: (queuedMessageId: string) => void;
  onRestoreTask?: (taskId: string) => void;
  onSendPrompt: (prompt?: string) => void;
  onPermissionPolicyChange?: (policy: import("@openaide/app-shell-contracts").TaskPermissionPolicy) => Promise<void>;
  onSelectConfigOption: (configId: string, value: ConfigOptionCurrentValue) => void;
  permissionResponses: AppState["permissionResponses"];
  planDrawerOpen?: boolean;
  liveTextPresentation?: TaskLiveTextPresentation;
  questionResponses?: AppState["questionResponses"];
  startupConfigOptions?: ConfigOptionsCatalog;
  snapshot: TaskSnapshot;
  savedScrollState?: TaskChatScrollState;
  taskInput: TaskComposerInput;
  toolDetails: AppState["toolDetails"];
  submitShortcut: AppPreferencesRecord["composer_submit_shortcut"];
  showWorkspaceContext?: boolean;
}) {
  const quoteRequestSequence = useRef(0);
  const [quoteRequest, setQuoteRequest] = useState<{ id: number; taskId: string; text: string }>();
  const fileViewerEnabled = currentFrontendShell()?.fileViewer === true && fileViewerConnection !== undefined;
  const fileViewer = useTaskFileViewer({
    connection: fileViewerEnabled ? fileViewerConnection : undefined,
    enabled: fileViewerEnabled,
    taskId: snapshot.task.task_id,
  });
  const openAgentFile = useCallback((path: string, line?: number) => {
    void fileViewer.openPath(path, line);
  }, [fileViewer.openPath]);
  const queueOverlayRef = useCallback((node: HTMLDivElement | null) => (
    node ? installTaskQueueOverlayClearance(node) : undefined
  ), []);
  const inputPending = taskInput.pending?.state === "sending";
  const recovery = taskAgentRecovery(
    snapshot.task.agent_id,
    activeTask?.agent_name ?? snapshot.task.agent_name,
    agents,
    snapshot.preparation,
  );
  const subagents = useSubagentSessions({
    connection: subagentConnection,
    enabled: backendReady,
    taskId: snapshot.task.task_id,
  });
  useEffect(() => {
    if (!subagents.selectedSubagentId) return;
    const returnToMain = (event: KeyboardEvent) => {
      if (!event.altKey || event.key !== "ArrowLeft" || event.ctrlKey || event.metaKey || event.shiftKey) {
        return;
      }
      event.preventDefault();
      subagents.selectSubagent(undefined);
    };
    window.addEventListener("keydown", returnToMain);
    return () => window.removeEventListener("keydown", returnToMain);
  }, [subagents.selectSubagent, subagents.selectedSubagentId]);
  const mainChat = useMemo(() => renderedChat(snapshot, chatPageState), [chatPageState, snapshot]);
  const childChat = useMemo(() => {
    if (!subagents.selected) return undefined;
    const history = subagents.history;
    return {
      items: (history?.chat.items ?? []).map((item) => mapProtocolChatItem(item, snapshot.task.updated_at)),
      hasBefore: history?.chat.hasMoreBefore === true,
      beforeCursor: history?.chat.startCursor ?? undefined,
      pending: history === undefined,
      error: history?.availability === "unavailable" ? "This subagent history is unavailable." : undefined,
    };
  }, [snapshot.task.updated_at, subagents.history, subagents.selected]);
  const chat = childChat ?? mainChat;
  const chatItems = useMemo(() => [
    ...chat.items,
    ...snapshot.active_requests,
  ], [chat.items, snapshot.active_requests]);
  const visiblePlan = useMemo(() => {
    if (!subagents.selected) return snapshot.current_plan;
    const plan = subagents.history?.currentPlan;
    return plan ? {
      entries: plan.entries.map((entry) => ({
        content: entry.content,
        priority: entry.priority,
        status: entry.status === "inProgress" ? "in_progress" as const : entry.status,
      })),
    } : undefined;
  }, [snapshot.current_plan, subagents.history?.currentPlan, subagents.selected]);
  const turnBusy = snapshot.task.status === "active";
  const queueAvailable = snapshot.task.status === "active"
    || snapshot.task.status === "waiting"
    || snapshot.task.status === "stopping";
  const workspaceAvailable = snapshot.task.workspace_available !== false;
  const imageAttachmentsAllowed = snapshot.input_capabilities?.image === true;
  const imageAttachments = taskInput.context.filter((attachment) => attachment.kind === "image");
  const resourceAttachments = taskInput.context.filter((attachment) => attachment.kind !== "image");
  const attachmentsSendable = appServerComposerImages(imageAttachments) !== undefined
    && appServerAttachmentHandles(resourceAttachments) !== undefined
    && (imageAttachments.length === 0 || imageAttachmentsAllowed);
  const baseAvailability = composerAvailability({
    allowEditingWhileSendBlocked: true,
    archived,
    attachmentsReady: attachmentsSendable,
    attachmentsBlockedMessage: imageAttachments.length > 0 && !imageAttachmentsAllowed
      ? "This Agent does not accept images."
      : "Attached context is not ready to send.",
    blockedPlaceholder: snapshot.task.status === "waiting"
      ? "Draft follow-up while input is pending."
      : snapshot.task.status === "active" ? "Send a follow-up" : undefined,
    connectionStatus: backendReady ? "ready" : backendConnectionState?.status ?? "connecting",
    contextReady: workspaceAvailable,
    contextPlaceholder: "Task workspace is unavailable. Restore it before sending.",
    readyPlaceholder: "Send follow-up",
    sendCapability: snapshot.send_capability,
    submitPendingLabel: "Sending message",
    submitting: inputPending,
  });
  const availability = taskInput.queueTake
    ? {
        ...baseAvailability,
        canEdit: false,
        submissionAllowed: false,
        submissionBlockedMessage: "Moving queued message to Composer.",
        placeholder: "Moving queued message to Composer.",
      }
    : baseAvailability;
  const quoteAvailable = availability.canEdit && !archived && !recovery;
  const requestQuote = useCallback((text: string) => {
    if (!quoteAvailable) return;
    subagents.selectSubagent(undefined);
    quoteRequestSequence.current += 1;
    setQuoteRequest({
      id: quoteRequestSequence.current,
      taskId: snapshot.task.task_id,
      text,
    });
  }, [quoteAvailable, snapshot.task.task_id, subagents.selectSubagent]);
  const canSubmit = composerCanSubmit(availability, taskInput.prompt, taskInput.context.length);
  const queue = useMemo(() => {
    const current = snapshot.message_queue ?? { revision: 0, items: [] };
    const take = taskInput.queueTake;
    if (!take || current.items.some((item) => item.queued_message_id === take.item.queued_message_id)) {
      return current;
    }
    const items = [...current.items];
    items.splice(Math.min(take.index, items.length), 0, take.item);
    return { ...current, items };
  }, [snapshot.message_queue, taskInput.queueTake]);
  const composerEmpty = taskInput.prompt.length === 0 && taskInput.context.length === 0;
  const completedPlanSteps = visiblePlan?.entries.filter((entry) => entry.status === "completed").length ?? 0;
  const taskConfigOptions = startupConfigOptions ?? snapshot.agent_config;
  const [showHistoryUpdated, setShowHistoryUpdated] = useState(false);
  const [reloadPending, setReloadPending] = useState(false);
  const [reloadError, setReloadError] = useState<string>();
  const historyUpdateKey = `${snapshot.task.task_id}:${snapshot.history_sync.generation}`;
  const initialHistoryUpdateKey = snapshot.history_sync.state === "updated"
    ? historyUpdateKey
    : undefined;
  // A terminal state in the opening baseline describes earlier work. Only a transition
  // observed by this mounted Task surface should interrupt the live status with a notice.
  const announcedHistoryUpdate = useRef<string | undefined>(initialHistoryUpdateKey);
  const observedHistoryTask = useRef(snapshot.task.task_id);
  useEffect(() => {
    if (observedHistoryTask.current !== snapshot.task.task_id) {
      observedHistoryTask.current = snapshot.task.task_id;
      announcedHistoryUpdate.current = snapshot.history_sync.state === "updated"
        ? historyUpdateKey
        : undefined;
      setShowHistoryUpdated(false);
      return undefined;
    }
    if (snapshot.history_sync.state !== "updated") {
      setShowHistoryUpdated(false);
      return undefined;
    }
    if (announcedHistoryUpdate.current === historyUpdateKey) {
      setShowHistoryUpdated(false);
      return undefined;
    }
    announcedHistoryUpdate.current = historyUpdateKey;
    setShowHistoryUpdated(true);
    const timer = window.setTimeout(() => setShowHistoryUpdated(false), 2_000);
    return () => window.clearTimeout(timer);
  }, [snapshot.history_sync.generation, snapshot.history_sync.state, snapshot.task.task_id]);
  useEffect(() => {
    if (snapshot.history_sync.state !== "reloadAvailable") {
      setReloadPending(false);
      setReloadError(undefined);
    }
  }, [snapshot.history_sync.state]);
  const seenPlanForTask = useRef({
    hasPlan: Boolean(visiblePlan),
    taskId: snapshot.task.task_id,
  });
  useEffect(() => {
    if (!visiblePlan) {
      resetAgentPlanDisclosure(snapshot.task.task_id);
      resetAgentPlanDisclosure(`${snapshot.task.task_id}:column`);
      resetAgentPlanDisclosure(`${snapshot.task.task_id}:drawer`);
    }
  }, [visiblePlan, snapshot.task.task_id]);
  useEffect(() => {
    const seen = seenPlanForTask.current;
    const hasPlan = Boolean(visiblePlan);
    if (seen.taskId !== snapshot.task.task_id) {
      seen.taskId = snapshot.task.task_id;
      seen.hasPlan = hasPlan;
      return;
    }
    if (hasPlan && !seen.hasPlan) onPlanDrawerOpenChange?.(true);
    seen.hasPlan = hasPlan;
  }, [onPlanDrawerOpenChange, visiblePlan, snapshot.task.task_id]);
  const showCurrentHistoryUpdated = showHistoryUpdated
    && announcedHistoryUpdate.current === historyUpdateKey;
  const timelineStatusLabel = subagents.selected
    ? subagents.history === undefined
      ? "Loading subagent history"
      : subagents.history.availability === "waitingForActivity"
        ? "Waiting for subagent activity"
        : undefined
    : taskWorkingStatusLabel(
      chatItems,
      snapshot.task.status,
      inputPending,
      snapshot.history_sync.state === "updated" && !showCurrentHistoryUpdated
        ? { state: "idle", generation: snapshot.history_sync.generation }
        : snapshot.history_sync,
    );
  const timelineStatusKind = showCurrentHistoryUpdated && snapshot.history_sync.state === "updated"
    ? "notice"
      : snapshot.task.status === "waiting"
      ? "blocked"
    : "progress";
  const workingStartedAt = snapshot.active_turn_started_at;
  const timelineRows = useMemo(
    () => buildTaskChatTimelineRows({ archived, chat, items: chatItems, timelineStatusLabel }),
    [archived, chat, chatItems, timelineStatusLabel],
  );
  const timelineRowKeys = useMemo(() => timelineRows.map((row) => row.key), [timelineRows]);
  const taskSelection = {
    agentId: snapshot.task.agent_id,
    agentLabel: activeTask?.agent_name ?? snapshot.task.agent_name,
    isolation: snapshot.settings_summary.isolation,
    workspaceRoot: snapshot.task.workspace_root,
    workspaceLabel: workspaceLabel(snapshot.task.workspace_root),
  };
  const loadChatPage = useCurrentCallback(onLoadChatPage);
  const chatScroll = useTaskChatScroll({
    beforeCursor: chat.beforeCursor,
    hasEarlier: chat.hasBefore,
    historySyncState: subagents.selected ? "idle" : snapshot.history_sync.state === "reloadAvailable"
      ? "idle"
      : snapshot.history_sync.state,
    itemKeys: timelineRowKeys,
    latestMessageKey: chatItems.at(-1)?.message_id,
    onLoadEarlier: subagents.selected ? subagents.loadEarlier : loadChatPage,
    onScrollState: intents.recordScroll,
    pendingPrepend: chat.pending,
    savedScrollState: subagents.selected ? undefined : savedScrollState,
    taskId: subagents.selected
      ? `${snapshot.task.task_id}:${subagents.selected.subagentId}`
      : snapshot.task.task_id,
  });
  const loadToolImagePreview = useCurrentCallback(onLoadToolImagePreview ?? unavailableToolImagePreview);
  const subscribeToolDetail = useCurrentCallback(onSubscribeToolDetail);
  const respondToPermission = useCurrentCallback(onPermissionRespond);
  const respondToQuestion = useCurrentCallback((requestId: string, response: ElicitationResponse) => {
    onQuestionRespond?.(requestId, response);
  });
  const restoreTask = useCurrentCallback((taskId: string) => {
    onRestoreTask?.(taskId);
  });
  const canReloadNativeSession = snapshot.history_sync.state === "reloadAvailable"
    && snapshot.task.status === "inactive"
    && !archived
    && backendReady
    && backendConnectionState?.status !== "unavailable"
    && onReloadNativeSession !== undefined;

  const reloadNativeSession = async () => {
    if (!canReloadNativeSession || reloadPending || !onReloadNativeSession) return;
    setReloadPending(true);
    setReloadError(undefined);
    try {
      await onReloadNativeSession();
    } catch {
      // Transport and Agent errors may contain sensitive details; the notice is a
      // recovery surface, so it stays concise and lets the user retry explicitly.
      setReloadError("Unable to reload Task. Try again.");
    } finally {
      setReloadPending(false);
    }
  };

  const submit = (prompt: string) => {
    if (!canSubmit) return;
    chatScroll.jumpToLatest();
    onSendPrompt(prompt);
  };

  const taskSurface = (
    <section
      aria-label="Task chat"
      className="task-surface task-work-stack"
      data-desktop-window={desktopWindow?.platform}
      data-file-viewer={fileViewer.visible ? (fileViewer.collapsed ? "collapsed" : "open") : undefined}
      style={fileViewer.visible
        ? { ["--task-panel-ratio" as string]: String(fileViewer.splitRatio) }
        : undefined}
    >
      <div className="task-work-stack-header">
        <TaskHeader
          agentNavigation={(
            <SubagentNavigator
              entries={subagents.catalog?.entries ?? []}
              onSelect={subagents.selectSubagent}
              selectedId={subagents.selectedSubagentId}
              unseen={subagents.unseen}
            />
          )}
          agentId={snapshot.task.agent_id}
          agentName={activeTask?.agent_name ?? snapshot.task.agent_name}
          desktopWindow={desktopWindow}
          status={snapshot.task.status}
          title={activeTask?.title ?? snapshot.task.title}
          permissionPolicy={snapshot.permission_policy}
          onPermissionPolicyChange={onPermissionPolicyChange}
          permissionPolicyDisabled={archived || !taskMutationReady}
          permissionPolicyDisabledReason={archived
            ? "Archived Tasks are read-only."
            : !taskMutationReady
              ? "Permission handling is unavailable until this Task is connected."
              : undefined}
          workspaceRoot={snapshot.task.workspace_root}
          worktreeName={snapshot.task.worktree_name}
          gitRef={snapshot.task.git_ref}
          showWorkspaceContext={showWorkspaceContext}
        />
        <div className="task-work-stack-header-actions">
          {visiblePlan ? (
            <button
              aria-expanded={planDrawerOpen}
              aria-label={planDrawerOpen ? "Hide Plan" : "Open Plan"}
              className="task-plan-drawer-trigger"
              onClick={() => onPlanDrawerOpenChange?.(!planDrawerOpen)}
              type="button"
            >
              <ListTodo aria-hidden="true" size={14} />
              <span>Plan</span>
              <small>{completedPlanSteps}/{visiblePlan.entries.length}</small>
            </button>
          ) : null}
          <TaskPanelToggle
            collapsed={fileViewer.collapsed}
            onToggle={() => fileViewer.setCollapsed(!fileViewer.collapsed)}
            visible={fileViewer.visible && fileViewer.collapsed}
          />
        </div>
      </div>
      <div
        className="task-workbench"
        data-file-viewer={fileViewer.visible ? (fileViewer.collapsed ? "collapsed" : "open") : undefined}
      >
        <div className="chat-column task-conversation">
        {visiblePlan ? (
          <aside aria-label="Current plan" className="task-plan-column">
            <AgentPlanView
              defaultOpen
              key={`column:${snapshot.task.task_id}`}
              onDismiss={onClosePlan}
              plan={visiblePlan}
              taskId={`${snapshot.task.task_id}:column`}
              taskStatus={snapshot.task.status}
            />
          </aside>
        ) : null}
        <TaskChatTimeline
          archived={archived}
          canRestoreTask={onRestoreTask !== undefined}
          chat={chat}
          chatScroll={chatScroll}
          commandCatalog={snapshot.agent_commands}
          items={chatItems}
          liveTextPresentation={subagents.selected ? subagents.liveTextPresentation : liveTextPresentation}
          onLoadChatPage={loadChatPage}
          onLoadToolImagePreview={loadToolImagePreview}
          onOpenSubagent={subagents.selectSubagent}
          onPermissionRespond={respondToPermission}
          onQuestionRespond={respondToQuestion}
          onQuote={quoteAvailable ? requestQuote : undefined}
          onRestoreTask={restoreTask}
          onSubscribeToolDetail={subscribeToolDetail}
          permissionResponses={permissionResponses}
          questionResponses={questionResponses}
          rows={timelineRows}
          taskId={snapshot.task.task_id}
          taskStatus={snapshot.task.status}
          toolDetails={toolDetails}
          timelineStatusKind={timelineStatusKind}
          timelineStatusLabel={timelineStatusLabel}
          workingStartedAt={workingStartedAt}
        />
        {backendConnectionState?.status === "unavailable" ? (
          <div className="task-connection-notice" role="status" aria-live="polite">
            <span>Unable to refresh task.</span>
            <small>{backendConnectionState.message}</small>
            {backendConnectionState.status === "unavailable" && onRetryConnection ? (
              <button type="button" onClick={onRetryConnection}>Retry</button>
            ) : null}
          </div>
        ) : null}
        {canReloadNativeSession ? (
          <TaskSessionReloadNotice
            error={reloadError}
            onReload={reloadNativeSession}
            pending={reloadPending}
          />
        ) : null}
        {!workspaceAvailable ? <div className="task-workspace-unavailable" role="status">
          <CircleAlert size={15} />
          <span><strong>Task workspace unavailable</strong><small>History is still available. Restore the folder before sending.</small></span>
          <div className="task-workspace-recovery-actions">
            {snapshot.task.worktree_id ? <>
              <button onClick={() => void intents.refreshWorkspace()} type="button">Refresh</button>
              {onManageWorktrees && snapshot.task.project_id ? <button onClick={() => onManageWorktrees(snapshot.task.project_id!)} type="button">Manage worktrees</button> : null}
            </> : <>
              {onOpenProjectSettings ? <button onClick={onOpenProjectSettings} type="button">Project settings</button> : null}
              {onReconnectProject && snapshot.task.project_id ? <button onClick={() => onReconnectProject(snapshot.task.project_id!)} type="button">Reconnect folder</button> : null}
            </>}
          </div>
        </div> : null}
        {onRemoveQueueMessage && queue.items.length > 0 ? (
          <section aria-label="Queued messages" className="task-queue-anchor">
            <div className="task-queue-floating" ref={queueOverlayRef}>
              <TaskMessageQueueView
                editDisabled={!composerEmpty || Boolean(taskInput.queueTake)}
                extractingQueuedMessageId={taskInput.queueTake?.item.queued_message_id}
                extractionStage={taskInput.queueTake?.stage}
                queue={queue}
                onExpanded={chatScroll.pauseFollowing}
                onRemove={onRemoveQueueMessage}
                onTake={onTakeQueueMessage}
                onMove={onMoveQueueMessage}
                onSendNow={onSendQueueMessageNow}
              />
            </div>
          </section>
        ) : null}
        {subagents.selected ? (
          <div className="subagent-inspection-footer" role="note">
            <span className="subagent-inspection-context">
              Viewing <strong>{subagents.selected.name}</strong>
            </span>
            <button
              aria-keyshortcuts="Alt+ArrowLeft"
              onClick={() => subagents.selectSubagent(undefined)}
              title="Back to Main Agent (Alt+Left)"
              type="button"
            >
              Back to Main Agent
            </button>
          </div>
        ) : recovery && agentRecoveryActions ? <AgentRecoveryPanel
          actions={agentRecoveryActions}
          agent={recovery.agent}
          kind={recovery.kind}
        /> : (
          <ComposerWithContextUsage
            configOptions={taskConfigOptions}
            usage={snapshot.context_usage}
          >
            <Composer
              agentLocked
              agents={agents}
              attachments={taskInput.context}
              autoFocus
              availability={availability}
              configLocked={!backendReady || !configOptionsMutable(taskConfigOptions)}
              configOptions={taskConfigOptions}
              commandCatalog={snapshot.agent_commands}
              error={taskInput.error ?? taskInput.configError?.message ?? taskConfigOptions?.error}
              fileBrowser={fileBrowser}
              imageAttachmentsAllowed={imageAttachmentsAllowed}
              historyScopeKey={`task:${snapshot.task.task_id}`}
              loadComposerHistory={onLoadComposerHistory}
              focusRequestKey={`${snapshot.task.task_id}:${taskInput.acceptedQueueTakeId ?? ""}`}
              onCancel={
                backendReady && (turnBusy || inputPending)
                  ? onCancel
                  : undefined
              }
              onAddToQueue={!archived && queueAvailable ? onAddToQueue : undefined}
              onChange={intents.changePrompt}
              onUnsupportedImageAttachment={intents.reportAttachmentError}
              onRevealAttachment={onRevealAttachment}
              onRemoveAttachment={onRemoveAttachment}
              onRetryConfigOptions={onRetryConfigOptions}
              onSelectConfigOption={onSelectConfigOption}
              onSubmit={submit}
              prompt={taskInput.prompt}
              quoteRequest={quoteRequest?.taskId === snapshot.task.task_id ? quoteRequest : undefined}
              selection={taskSelection}
              submitShortcut={submitShortcut}
              submissionSettlementKey={taskInput.acceptedUserMessageId ?? taskInput.acceptedQueueRevision}
              showAgentSelector={false}
              showIsolationSelector={false}
            />
          </ComposerWithContextUsage>
        )}
        </div>
      </div>
      <FileViewerPanel
        collapsed={fileViewer.collapsed}
        onClose={fileViewer.closeTab}
        onOpenFromHandle={fileViewer.openFromHandle}
        onQuote={quoteAvailable ? requestQuote : () => undefined}
        onRefresh={(handle) => void fileViewer.refresh(handle)}
        onSelect={fileViewer.selectTab}
        onSplitRatio={fileViewer.setSplitRatio}
        onToggleCollapsed={() => fileViewer.setCollapsed(!fileViewer.collapsed)}
        splitRatio={fileViewer.splitRatio}
        tab={fileViewer.activeTab}
        tabs={fileViewer.tabs}
      />
      {visiblePlan ? (
        <>
          <div
            aria-hidden="true"
            className="task-plan-drawer-backdrop"
            onClick={() => onPlanDrawerOpenChange?.(false)}
          />
          <aside
            aria-hidden={!planDrawerOpen}
            aria-label="Current plan"
            className="task-plan-drawer"
            data-open={planDrawerOpen}
            inert={planDrawerOpen ? undefined : true}
          >
            <AgentPlanView
              collapsible={false}
              key={`drawer:${snapshot.task.task_id}`}
              onDismiss={onClosePlan}
              onHide={() => onPlanDrawerOpenChange?.(false)}
              plan={visiblePlan}
              taskId={`${snapshot.task.task_id}:drawer`}
              taskStatus={snapshot.task.status}
            />
          </aside>
        </>
      ) : null}
    </section>
  );

  if (!fileViewerEnabled) return taskSurface;
  return (
    <AgentFileOpenContext.Provider value={openAgentFile}>
      {taskSurface}
    </AgentFileOpenContext.Provider>
  );
}

async function unavailableToolImagePreview() {
  return undefined;
}

/** Keeps a callback interface stable while routing calls to the latest controller closure. */
function useCurrentCallback<Arguments extends unknown[], Result>(
  callback: (...args: Arguments) => Result,
) {
  const callbackRef = useRef(callback);
  callbackRef.current = callback;
  return useCallback((...args: Arguments) => callbackRef.current(...args), []);
}
