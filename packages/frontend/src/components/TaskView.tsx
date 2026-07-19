import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ArrowDown, Check, CircleAlert } from "lucide-react";
import type {
  AppPreferencesRecord,
  ChatMessage,
  ConfigOptionsCatalog,
  ElicitationResponse,
  TaskSnapshot,
  TaskSummary,
} from "@openaide/app-shell-contracts";
import { renderedChat } from "../state/chatPaging";
import type { AppState, TaskChatScrollState, TaskComposerInput, TaskLiveTextPresentation } from "../state/store";
import { ChatRow } from "./ChatMessageView";
import { Composer } from "./Composer";
import { composerAvailability, composerCanSubmit } from "./composerAvailability";
import { TaskHeader } from "./TaskHeader";
import { scrollTopAfterPrependedContent } from "./TaskViewModel";
import { taskWorkingStatusLabel, timestampMillis, workspaceLabel } from "./taskSurfaceHelpers";
import type { TaskFileBrowserCallbacks } from "./appControllerCallbackTypes";
import {
  permissionResponseForMessage,
  questionResponseForMessage,
} from "./taskChatPresentation";
import { useTaskChatScroll } from "./useTaskChatScroll";
import { appServerComposerImages } from "../state/composerOptions";
import { configOptionsMutable } from "../state/configOptionState";
import type { BackendConnectionState } from "./appControllerBackendLifecycle";
import type { AgentOption } from "../state/composerOptions";
import { AgentRecoveryPanel, taskAgentRecovery, type AgentRecoveryActions } from "./AgentRecovery";

export {
  scrollTopAfterPrependedContent,
} from "./TaskViewModel";
export {
  permissionResponseForMessage,
  questionResponseForMessage,
} from "./taskChatPresentation";

const RECONNECT_NOTICE_DELAY_MS = 1_000;

export type TaskViewIntents = {
  changePrompt: (prompt: string) => void;
  refreshWorkspace: () => Promise<void>;
  recordScroll: (scrollState: TaskChatScrollState) => void;
  reportAttachmentError: (message?: string) => void;
};

export function TaskLoadingView({ error, onRetry }: { error?: string; onRetry?: () => void }) {
  if (error) {
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
    <section className="task-surface task-loading" aria-label="Opening task">
      <div className="task-loading-status" role="status" aria-live="polite">
        <span className="working-status-dots" aria-hidden="true">
          <span />
          <span />
          <span />
        </span>
        <span>Opening task</span>
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
  backendReady,
  chatPageState,
  intents,
  onCancel,
  fileBrowser,
  onLoadChatPage,
  onManageWorktrees,
  onOpenProjectSettings,
  onSubscribeToolDetail,
  onPermissionRespond,
  onQuestionRespond,
  onReconnectProject,
  onRetryConnection,
  onRevealAttachment,
  onRemoveAttachment,
  onRestoreTask,
  onSendPrompt,
  onSelectConfigOption,
  permissionResponses,
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
  backendReady: boolean;
  chatPageState: AppState["chatPages"][string] | undefined;
  intents: TaskViewIntents;
  onCancel: () => void;
  fileBrowser?: TaskFileBrowserCallbacks;
  onLoadChatPage: (beforeCursor: string) => number | undefined;
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
  onRevealAttachment: (attachmentId: string) => Promise<void> | void;
  onRemoveAttachment: (attachmentId: string) => void;
  onRestoreTask?: (taskId: string) => void;
  onSendPrompt: (prompt?: string) => void;
  onSelectConfigOption: (configId: string, value: string) => void;
  permissionResponses: AppState["permissionResponses"];
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
  const inputPending = taskInput.pending?.state === "sending";
  const recovery = taskAgentRecovery(
    snapshot.task.agent_id,
    activeTask?.agent_name ?? snapshot.task.agent_name,
    agents,
    snapshot.preparation,
  );
  const chat = useMemo(() => renderedChat(snapshot, chatPageState), [chatPageState, snapshot]);
  const chatItems = useMemo(() => [
    ...chat.items,
    ...snapshot.active_requests,
  ], [chat.items, snapshot.active_requests]);
  const turnBusy = snapshot.task.status === "active";
  const workspaceAvailable = snapshot.task.workspace_available !== false;
  const imageAttachmentsAllowed = snapshot.input_capabilities?.image === true;
  const attachmentsSendable = taskInput.context.length === 0
    || (appServerComposerImages(taskInput.context) !== undefined
      && imageAttachmentsAllowed);
  const availability = composerAvailability({
    allowEditingWhileSendBlocked: true,
    archived,
    attachmentsReady: attachmentsSendable,
    attachmentsBlockedMessage: taskInput.context.length > 0 && !imageAttachmentsAllowed
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
  const canSubmit = composerCanSubmit(availability, taskInput.prompt, taskInput.context.length);
  const taskConfigOptions = startupConfigOptions ?? snapshot.agent_config;
  const [showHistoryUpdated, setShowHistoryUpdated] = useState(false);
  const [showReconnectNotice, setShowReconnectNotice] = useState(false);
  const announcedHistoryUpdate = useRef<string | undefined>(undefined);
  const reconnecting = backendConnectionState?.status === "reconnecting";
  useEffect(() => {
    if (!reconnecting) {
      setShowReconnectNotice(false);
      return undefined;
    }
    // Page unloads and brief stream replacement are normal. Keep Send blocked
    // immediately, but do not turn a sub-second resynchronization into an error.
    const timer = window.setTimeout(() => setShowReconnectNotice(true), RECONNECT_NOTICE_DELAY_MS);
    return () => window.clearTimeout(timer);
  }, [reconnecting]);
  useEffect(() => {
    if (snapshot.history_sync.state !== "updated") {
      setShowHistoryUpdated(false);
      return undefined;
    }
    const announcementKey = `${snapshot.task.task_id}:${snapshot.history_sync.generation}`;
    if (announcedHistoryUpdate.current === announcementKey) {
      setShowHistoryUpdated(false);
      return undefined;
    }
    announcedHistoryUpdate.current = announcementKey;
    setShowHistoryUpdated(true);
    const timer = window.setTimeout(() => setShowHistoryUpdated(false), 2_000);
    return () => window.clearTimeout(timer);
  }, [snapshot.history_sync.generation, snapshot.history_sync.state, snapshot.task.task_id]);
  const timelineStatusLabel = taskWorkingStatusLabel(
    chatItems,
    snapshot.task.status,
    inputPending,
    snapshot.history_sync.state === "updated" && !showHistoryUpdated
      ? { state: "idle", generation: snapshot.history_sync.generation }
      : snapshot.history_sync,
  );
  const timelineStatusKind = showHistoryUpdated && snapshot.history_sync.state === "updated"
    ? "notice"
    : snapshot.task.status === "waiting"
      ? "blocked"
    : "progress";
  const workingStartedAt = snapshot.active_turn_started_at;
  const taskSelection = {
    agentId: snapshot.task.agent_id,
    agentLabel: activeTask?.agent_name ?? snapshot.task.agent_name,
    isolation: snapshot.settings_summary.isolation,
    configOptions: snapshot.settings_summary.config_options ?? {},
    workspaceRoot: snapshot.task.workspace_root,
    workspaceLabel: workspaceLabel(snapshot.task.workspace_root),
  };
  const chatScroll = useTaskChatScroll({
    historySyncState: snapshot.history_sync.state,
    itemCount: chatItems.length,
    onScrollState: intents.recordScroll,
    pendingPrepend: chat.pending,
    prependRequestGeneration: chatPageState?.requestGeneration ?? 0,
    savedScrollState,
    taskId: snapshot.task.task_id,
  });
  const loadChatPage = useCurrentCallback(onLoadChatPage);
  const subscribeToolDetail = useCurrentCallback(onSubscribeToolDetail);
  const respondToPermission = useCurrentCallback(onPermissionRespond);
  const respondToQuestion = useCurrentCallback((requestId: string, response: ElicitationResponse) => {
    onQuestionRespond?.(requestId, response);
  });
  const restoreTask = useCurrentCallback((taskId: string) => {
    onRestoreTask?.(taskId);
  });

  const submit = (prompt: string) => {
    if (!canSubmit) return;
    chatScroll.jumpToLatest();
    onSendPrompt(prompt);
  };

  return (
    <section className="task-surface" aria-label="Task chat">
      <TaskHeader
        agentId={snapshot.task.agent_id}
        agentName={activeTask?.agent_name ?? snapshot.task.agent_name}
        status={snapshot.task.status}
        title={activeTask?.title ?? snapshot.task.title}
        workspaceRoot={snapshot.task.workspace_root}
        worktreeName={snapshot.task.worktree_name}
        gitRef={snapshot.task.git_ref}
        showWorkspaceContext={showWorkspaceContext}
      />
      <div className="chat-column">
        <TaskChatTimeline
          archived={archived}
          canRestoreTask={onRestoreTask !== undefined}
          chat={chat}
          chatScroll={chatScroll}
          commandCatalog={snapshot.agent_commands}
          items={chatItems}
          liveTextPresentation={liveTextPresentation}
          onLoadChatPage={loadChatPage}
          onPermissionRespond={respondToPermission}
          onQuestionRespond={respondToQuestion}
          onRestoreTask={restoreTask}
          onSubscribeToolDetail={subscribeToolDetail}
          permissionResponses={permissionResponses}
          questionResponses={questionResponses}
          taskId={snapshot.task.task_id}
          taskStatus={snapshot.task.status}
          toolDetails={toolDetails}
          timelineStatusKind={timelineStatusKind}
          timelineStatusLabel={timelineStatusLabel}
          workingStartedAt={workingStartedAt}
        />
        {(reconnecting && showReconnectNotice) || backendConnectionState?.status === "unavailable" ? (
          <div className="task-connection-notice" role="status" aria-live="polite">
            <span>
              {reconnecting
                ? "Reconnecting to App Server."
                : "Unable to refresh task."}
            </span>
            <small>{reconnecting ? "App Server is temporarily unavailable." : backendConnectionState.message}</small>
            {backendConnectionState.status === "unavailable" && onRetryConnection ? (
              <button type="button" onClick={onRetryConnection}>Retry</button>
            ) : null}
          </div>
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
        {recovery && agentRecoveryActions ? <AgentRecoveryPanel
          actions={agentRecoveryActions}
          agent={recovery.agent}
          kind={recovery.kind}
        /> : <Composer
          agentLocked
          attachments={taskInput.context}
          autoFocus
          availability={availability}
          configLocked={!backendReady || !configOptionsMutable(taskConfigOptions)}
          configOptions={taskConfigOptions}
          commandCatalog={snapshot.agent_commands}
          error={taskInput.error ?? taskInput.configError?.message ?? taskConfigOptions?.error}
          fileBrowser={fileBrowser}
          imageAttachmentsAllowed={imageAttachmentsAllowed}
          focusRequestKey={snapshot.task.task_id}
          onCancel={
            backendReady && (turnBusy || inputPending)
              ? onCancel
              : undefined
          }
          onChange={intents.changePrompt}
          onUnsupportedImageAttachment={intents.reportAttachmentError}
          onRevealAttachment={onRevealAttachment}
          onRemoveAttachment={onRemoveAttachment}
          onSelectConfigOption={onSelectConfigOption}
          onSubmit={submit}
          prompt={taskInput.prompt}
          selection={taskSelection}
          submitShortcut={submitShortcut}
          submissionSettlementKey={taskInput.acceptedUserMessageId}
          showAgentSelector={false}
          showIsolationSelector={false}
        />}
      </div>
    </section>
  );
}

type TaskChatTimelineProps = {
  archived: boolean;
  canRestoreTask: boolean;
  chat: ReturnType<typeof renderedChat>;
  chatScroll: ReturnType<typeof useTaskChatScroll>;
  commandCatalog: TaskSnapshot["agent_commands"];
  items: ChatMessage[];
  liveTextPresentation?: TaskLiveTextPresentation;
  onLoadChatPage: (beforeCursor: string) => number | undefined;
  onPermissionRespond: (requestId: string, optionId: string) => void;
  onQuestionRespond: (requestId: string, response: ElicitationResponse) => void;
  onRestoreTask: (taskId: string) => void;
  onSubscribeToolDetail: (artifactId: string) => () => void;
  permissionResponses: AppState["permissionResponses"];
  questionResponses: AppState["questionResponses"];
  taskId: string;
  taskStatus: TaskSnapshot["task"]["status"];
  toolDetails: AppState["toolDetails"];
  timelineStatusKind: "blocked" | "notice" | "progress";
  timelineStatusLabel?: string;
  workingStartedAt?: string;
};

// Composer drafts update independently from authoritative Chat and must not invalidate its rows.
const TaskChatTimeline = memo(function TaskChatTimeline({
  archived,
  canRestoreTask,
  chat,
  chatScroll,
  commandCatalog,
  items,
  liveTextPresentation,
  onLoadChatPage,
  onPermissionRespond,
  onQuestionRespond,
  onRestoreTask,
  onSubscribeToolDetail,
  permissionResponses,
  questionResponses,
  taskId,
  taskStatus,
  toolDetails,
  timelineStatusKind,
  timelineStatusLabel,
  workingStartedAt,
}: TaskChatTimelineProps) {
  const latestTextMessageIds = latestTextMessageIdsByChannel(items);
  return (
    <div className="message-list-shell">
      <div
        className="message-list"
        onKeyDown={chatScroll.onKeyDown}
        onPointerCancel={chatScroll.onPointerCancel}
        onPointerDown={chatScroll.onPointerDown}
        onPointerUp={chatScroll.onPointerUp}
        onScroll={chatScroll.onScroll}
        onWheel={chatScroll.onWheel}
        ref={chatScroll.messageListRef}
      >
        {archived ? (
          <div className="archived-task-notice" role="status">
            <span>Archived task. Restore it to send a follow-up.</span>
            {canRestoreTask ? (
              <button type="button" onClick={() => onRestoreTask(taskId)}>
                Restore
              </button>
            ) : null}
          </div>
        ) : null}
        {chat.hasBefore ? (
          <div className="load-earlier-row">
            <button
              disabled={chat.pending || !chat.beforeCursor}
              onClick={() => {
                if (!chat.beforeCursor || chat.pending) return;
                const requestGeneration = onLoadChatPage(chat.beforeCursor);
                if (requestGeneration !== undefined) {
                  chatScroll.capturePrependAnchor(requestGeneration);
                }
              }}
              type="button"
            >
              {chat.pending ? "Loading earlier" : "Load earlier"}
            </button>
          </div>
        ) : null}
        {chat.error ? <p className="chat-system">{chat.error}</p> : null}
        {items.map((message) => (
          <ChatRow
            key={chatRowKey(message)}
            message={message}
            liveTextEventCursor={liveTextCursorForMessage(liveTextPresentation, latestTextMessageIds, message)}
            presentLiveText={taskStatus === "active" || taskStatus === "waiting" || taskStatus === "stopping"}
            taskId={taskId}
            toolDetails={toolDetails}
            onSubscribeToolDetail={onSubscribeToolDetail}
            permissionResponse={permissionResponseForMessage(message.message, permissionResponses)}
            onPermissionRespond={onPermissionRespond}
            onQuestionRespond={onQuestionRespond}
            questionResponse={questionResponseForMessage(message.message, questionResponses)}
            commandCatalog={commandCatalog}
          />
        ))}
        {timelineStatusLabel ? (
          <TimelineStatus kind={timelineStatusKind} label={timelineStatusLabel} startedAt={workingStartedAt} />
        ) : null}
      </div>
      {chatScroll.showJumpToLatest ? (
        <button
          aria-label="Jump to latest message"
          className="jump-to-latest"
          onClick={chatScroll.jumpToLatest}
          title="Jump to latest"
          type="button"
        >
          <ArrowDown aria-hidden="true" size={14} />
        </button>
      ) : null}
    </div>
  );
});

function liveTextCursorForMessage(
  presentation: TaskLiveTextPresentation | undefined,
  latestMessageIds: Partial<Record<"agent" | "thought", string>>,
  message: ChatMessage,
) {
  if (message.message.kind !== "agent_message") return undefined;
  if (latestMessageIds[message.message.role] !== message.message_id) return undefined;
  const signal = presentation?.[message.message.role];
  return signal?.messageId === message.message_id ? signal.eventCursor : undefined;
}

function latestTextMessageIdsByChannel(items: ChatMessage[]) {
  const latest: Partial<Record<"agent" | "thought", string>> = {};
  for (let index = items.length - 1; index >= 0; index -= 1) {
    const item = items[index];
    if (item?.message.kind !== "agent_message") continue;
    latest[item.message.role] ??= item.message_id;
    if (latest.agent && latest.thought) break;
  }
  return latest;
}

/** Keeps a callback interface stable while routing calls to the latest controller closure. */
function useCurrentCallback<Arguments extends unknown[], Result>(
  callback: (...args: Arguments) => Result,
) {
  const callbackRef = useRef(callback);
  callbackRef.current = callback;
  return useCallback((...args: Arguments) => callbackRef.current(...args), []);
}

export function chatRowKey(message: ChatMessage) {
  return message.message_id;
}

function TimelineStatus({
  kind,
  label,
  onRetry,
  startedAt,
}: {
  kind: "blocked" | "notice" | "progress";
  label: string;
  onRetry?: () => void;
  startedAt?: string;
}) {
  const elapsedSeconds = useElapsedSeconds(kind === "progress" ? startedAt : undefined);
  const visibleElapsed = elapsedSeconds !== undefined && elapsedSeconds >= 5
    ? formatElapsedDuration(elapsedSeconds)
    : undefined;
  return (
    <div className={`working-status working-status-${kind}`}>
      {kind === "progress" ? (
        <span className="working-status-dots" aria-hidden="true">
          <span />
          <span />
          <span />
        </span>
      ) : kind === "notice" ? (
        <Check aria-hidden="true" className="working-status-notice-icon" size={14} />
      ) : <CircleAlert aria-hidden="true" className="working-status-blocked-icon" size={14} />}
      <span className="working-status-label" role="status" aria-live="polite">{label}</span>
      {visibleElapsed && elapsedSeconds !== undefined ? (
        <>
          <span className="working-status-duration-separator" aria-hidden="true" />
          <time
            aria-label={`Elapsed time ${elapsedDurationLabel(elapsedSeconds)}`}
            className="working-status-duration"
            dateTime={`PT${elapsedSeconds}S`}
          >
            {visibleElapsed}
          </time>
        </>
      ) : null}
      {onRetry ? <button type="button" onClick={onRetry}>Retry</button> : null}
    </div>
  );
}

/** Keeps clock ticks inside the live footer so the surrounding Chat timeline stays stable. */
function useElapsedSeconds(startedAt?: string) {
  const startedAtMs = startedAt ? timestampMillis(startedAt) : Number.NaN;
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    setNow(Date.now());
    if (Number.isNaN(startedAtMs)) return undefined;
    const timer = globalThis.setInterval(() => setNow(Date.now()), 1_000);
    return () => globalThis.clearInterval(timer);
  }, [startedAtMs]);
  if (Number.isNaN(startedAtMs)) return undefined;
  return Math.max(0, Math.floor((now - startedAtMs) / 1_000));
}

export function formatElapsedDuration(totalSeconds: number) {
  const hours = Math.floor(totalSeconds / 3_600);
  const minutes = Math.floor((totalSeconds % 3_600) / 60);
  const seconds = totalSeconds % 60;
  if (hours > 0) return `${hours}h ${String(minutes).padStart(2, "0")}m`;
  return `${minutes}:${String(seconds).padStart(2, "0")}`;
}

function elapsedDurationLabel(totalSeconds: number) {
  const hours = Math.floor(totalSeconds / 3_600);
  const minutes = Math.floor((totalSeconds % 3_600) / 60);
  const seconds = totalSeconds % 60;
  return [
    hours ? `${hours} hour${hours === 1 ? "" : "s"}` : undefined,
    minutes ? `${minutes} minute${minutes === 1 ? "" : "s"}` : undefined,
    `${seconds} second${seconds === 1 ? "" : "s"}`,
  ].filter(Boolean).join(" ");
}
