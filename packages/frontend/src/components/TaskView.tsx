import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ArrowDown } from "lucide-react";
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
import { taskWorkingStatusLabel, workspaceLabel } from "./taskSurfaceHelpers";
import type { TaskFileBrowserCallbacks } from "./appControllerCallbackTypes";
import {
  permissionResponseForMessage,
  questionResponseForMessage,
} from "./taskChatPresentation";
import { useTaskChatScroll } from "./useTaskChatScroll";
import { appServerAttachmentHandles } from "../state/composerOptions";
import { configOptionsMutable } from "../state/configOptionState";
import type { BackendConnectionState } from "./appControllerBackendLifecycle";

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
  archived = false,
  backendConnectionState,
  backendReady,
  chatPageState,
  intents,
  onCancel,
  fileBrowser,
  onLoadChatPage,
  onSubscribeToolDetail,
  onPermissionRespond,
  onQuestionRespond,
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
  archived?: boolean;
  backendConnectionState?: BackendConnectionState;
  backendReady: boolean;
  chatPageState: AppState["chatPages"][string] | undefined;
  intents: TaskViewIntents;
  onCancel: () => void;
  fileBrowser?: TaskFileBrowserCallbacks;
  onLoadChatPage: (beforeCursor: string) => number | undefined;
  onSubscribeToolDetail: (artifactId: string) => () => void;
  onPermissionRespond: (
    requestId: string,
    optionId: string,
  ) => void;
  onQuestionRespond?: (requestId: string, response: ElicitationResponse) => void;
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
  const chat = useMemo(() => renderedChat(snapshot, chatPageState), [chatPageState, snapshot]);
  const chatItems = useMemo(() => [
    ...chat.items,
    ...snapshot.active_requests,
  ], [chat.items, snapshot.active_requests]);
  const turnBusy = snapshot.task.status === "active";
  const attachmentsSendable = taskInput.context.length === 0
    || appServerAttachmentHandles(taskInput.context) !== undefined;
  const availability = composerAvailability({
    allowEditingWhileSendBlocked: true,
    archived,
    attachmentsReady: attachmentsSendable,
    blockedPlaceholder: snapshot.task.status === "waiting"
      ? "Draft follow-up while input is pending."
      : snapshot.task.status === "active" ? "Send a follow-up" : undefined,
    connectionStatus: backendReady ? "ready" : backendConnectionState?.status ?? "connecting",
    contextReady: true,
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
  const workingLabel = taskWorkingStatusLabel(
    chatItems,
    snapshot.task.status,
    inputPending,
    snapshot.history_sync.state === "updated" && !showHistoryUpdated
      ? { state: "idle", generation: snapshot.history_sync.generation }
      : snapshot.history_sync,
  );
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
          workingLabel={workingLabel}
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
        <Composer
          agentLocked
          attachments={taskInput.context}
          autoFocus
          availability={availability}
          configLocked={!backendReady || !configOptionsMutable(taskConfigOptions)}
          configOptions={taskConfigOptions}
          commandCatalog={snapshot.agent_commands}
          error={taskInput.error ?? taskConfigOptions?.error}
          fileBrowser={fileBrowser}
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
        />
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
  workingLabel?: string;
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
  workingLabel,
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
        {workingLabel ? <WorkingStatus label={workingLabel} /> : null}
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

function WorkingStatus({ label, onRetry }: { label: string; onRetry?: () => void }) {
  return (
    <div className="working-status" role="status" aria-live="polite">
      <span className="working-status-dots" aria-hidden="true">
        <span />
        <span />
        <span />
      </span>
      <span>{label}</span>
      {onRetry ? <button type="button" onClick={onRetry}>Retry</button> : null}
    </div>
  );
}
