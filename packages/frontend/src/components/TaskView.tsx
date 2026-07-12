import { useCallback, useEffect, useRef, useState } from "react";
import type { Dispatch } from "react";
import { ArrowDown } from "lucide-react";
import type {
  AppPreferencesRecord,
  ChatMessage,
  ConfigOptionsCatalog,
  ElicitationResponse,
  PermissionDecision,
  TaskSnapshot,
  TaskSummary,
} from "@openaide/app-shell-contracts";
import type { AppAction } from "../state/appReducer";
import { renderedChat } from "../state/chatPaging";
import type { AppState, TaskChatScrollState, TaskComposerInput } from "../state/store";
import { ChatRow } from "./ChatMessageView";
import { Composer } from "./Composer";
import { TaskHeader } from "./TaskHeader";
import {
  scrollTopAfterPrependedContent,
  taskComposerAvailability,
} from "./TaskViewModel";
import { taskWorkingStatusLabel, workspaceLabel } from "./taskSurfaceHelpers";
import type { TaskFileBrowserCallbacks } from "./appControllerCallbackTypes";
import {
  chatItemsWithAppServerPermissions,
  chatItemsWithAppServerQuestions,
  permissionResponseForMessage,
  questionResponseForMessage,
} from "./taskChatPresentation";
import { useTaskChatScroll } from "./useTaskChatScroll";
import { appServerAttachmentHandles } from "../state/composerOptions";
import { configOptionsMutable } from "../state/configOptionState";
import type { BackendConnectionState } from "./appControllerBackendLifecycle";

export {
  scrollTopAfterPrependedContent,
  taskComposerAvailability,
} from "./TaskViewModel";
export {
  chatItemsWithAppServerPermissions,
  chatItemsWithAppServerQuestions,
  permissionResponseForMessage,
  questionResponseForMessage,
} from "./taskChatPresentation";

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
  appServerPermissionRequests,
  appServerQuestionRequests = {},
  archived = false,
  backendConnectionState,
  backendReady,
  chatPageState,
  dispatch,
  onCancel,
  fileBrowser,
  onLoadChatPage,
  onLoadToolDetail,
  onPermissionRespond,
  onQuestionRespond,
  onRetryConnection,
  onRevealAttachment,
  onRemoveAttachment,
  onRetryHistory,
  onRestoreTask,
  onSendPrompt,
  onSelectConfigOption,
  permissionResponses,
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
  appServerPermissionRequests: AppState["appServerPermissionRequests"];
  appServerQuestionRequests?: AppState["appServerQuestionRequests"];
  archived?: boolean;
  backendConnectionState?: BackendConnectionState;
  backendReady: boolean;
  chatPageState: AppState["chatPages"][string] | undefined;
  dispatch: Dispatch<AppAction>;
  onCancel: () => void;
  fileBrowser?: TaskFileBrowserCallbacks;
  onLoadChatPage: (beforeCursor: string) => number | undefined;
  onLoadToolDetail: (artifactId: string, refresh?: boolean) => void;
  onPermissionRespond: (
    requestId: string,
    optionId: string,
    decision: PermissionDecision,
    source?: "agent" | "appServer",
  ) => void;
  onQuestionRespond?: (requestId: string, response: ElicitationResponse) => void;
  onRetryConnection?: () => void;
  onRevealAttachment: (attachmentId: string) => Promise<void> | void;
  onRemoveAttachment: (attachmentId: string) => void;
  onRetryHistory?: () => void;
  onRestoreTask?: (taskId: string) => void;
  onSendPrompt: (prompt?: string) => void;
  onSelectConfigOption: (configId: string, value: string) => void;
  permissionResponses: AppState["permissionResponses"];
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
  const inputUncertain = taskInput.pending?.state === "uncertain";
  const chat = renderedChat(snapshot, chatPageState);
  const chatItems = chatItemsWithAppServerQuestions(
    chatItemsWithAppServerPermissions(
      chat.items,
      appServerPermissionRequests,
      snapshot.task.task_id,
    ),
    appServerQuestionRequests,
    snapshot.task.task_id,
  );
  const latestChatItem = chatItems.at(-1);
  const preparationBlocked = chatItems.some((item) => item.message_id === "app-server-preparation");
  const turnBusy = snapshot.task.status === "active";
  const composerAvailability = taskComposerAvailability({
    archived,
    backendReady,
    connectionStatus: backendConnectionState?.status,
    inputPending,
    inputUncertain,
    preparationBlocked,
    sendCapabilityState: snapshot.send_capability.state,
    taskStatus: snapshot.task.status,
  });
  const composerDisabled = composerAvailability.editingDisabled;
  const taskConfigOptions = startupConfigOptions ?? snapshot.agent_config;
  const attachmentsSendable = taskInput.context.length === 0
    || appServerAttachmentHandles(taskInput.context) !== undefined;
  const canSend = !composerAvailability.sendDisabled && attachmentsSendable;
  const [showHistoryUpdated, setShowHistoryUpdated] = useState(false);
  const announcedHistoryUpdate = useRef<string | undefined>(undefined);
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
  const recordTaskScroll = useCallback((scrollState: TaskChatScrollState) => {
    dispatch({ type: "taskScroll:record", taskId: snapshot.task.task_id, scrollState });
  }, [dispatch, snapshot.task.task_id]);
  const chatScroll = useTaskChatScroll({
    historySyncState: snapshot.history_sync.state,
    itemCount: chat.items.length,
    onScrollState: recordTaskScroll,
    pendingPrepend: chat.pending,
    prependRequestGeneration: chatPageState?.requestGeneration ?? 0,
    savedScrollState,
    taskId: snapshot.task.task_id,
  });

  const submit = (prompt: string) => {
    if (!canSend) return;
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
              {onRestoreTask ? (
                <button type="button" onClick={() => onRestoreTask(snapshot.task.task_id)}>
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
          {chatItems.map((message) => (
            <ChatRow
              key={chatRowKey(message)}
              message={message}
              showStreamingCaret={message === latestChatItem}
              taskId={snapshot.task.task_id}
              toolDetails={toolDetails}
              onLoadToolDetail={onLoadToolDetail}
              permissionResponse={permissionResponseForMessage(message.message, permissionResponses)}
              onPermissionRespond={onPermissionRespond}
              onQuestionRespond={onQuestionRespond}
              questionResponse={questionResponseForMessage(message.message, questionResponses)}
              commandCatalog={snapshot.agent_commands}
            />
          ))}
            {workingLabel ? (
              <WorkingStatus
                label={workingLabel}
                onRetry={snapshot.history_sync.state === "failed" ? onRetryHistory : undefined}
              />
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
        {backendConnectionState?.status === "reconnecting" || backendConnectionState?.status === "unavailable" ? (
          <div className="task-connection-notice" role="status" aria-live="polite">
            <span>
              {backendConnectionState.status === "reconnecting"
                ? "Reconnecting to App Server."
                : "Unable to refresh task."}
            </span>
            <small>{backendConnectionState.message}</small>
            {backendConnectionState.status === "unavailable" && onRetryConnection ? (
              <button type="button" onClick={onRetryConnection}>Retry</button>
            ) : null}
          </div>
        ) : null}
        <Composer
          agentLocked
          attachments={taskInput.context}
          autoFocus
          configLocked={!backendReady || !configOptionsMutable(taskConfigOptions)}
          configOptions={taskConfigOptions}
          commandCatalog={snapshot.agent_commands}
          disabled={composerDisabled}
          error={taskInput.error ?? taskConfigOptions?.error}
          fileBrowser={fileBrowser}
          focusRequestKey={snapshot.task.task_id}
          onCancel={
            backendReady && (turnBusy || inputPending)
              ? onCancel
              : undefined
          }
          onChange={(prompt) => dispatch({ type: "taskInput:prompt", taskId: snapshot.task.task_id, prompt })}
          onUnsupportedImageAttachment={(message) =>
            dispatch({
              type: "taskInput:error",
              taskId: snapshot.task.task_id,
              message: message ?? "Unable to attach image.",
            })
          }
          onRevealAttachment={onRevealAttachment}
          onRemoveAttachment={onRemoveAttachment}
          onSelectConfigOption={onSelectConfigOption}
          onSubmit={submit}
          placeholder={composerAvailability.placeholder}
          prompt={taskInput.prompt}
          selection={taskSelection}
          submitShortcut={submitShortcut}
          submitDisabled={!canSend}
          submitActionLabel={inputUncertain ? "Retry sending exact message" : undefined}
          submitPending={inputPending}
          submitPendingLabel="Sending message"
          submitRequiresText={!snapshot.send_capability.attachment_only}
          submissionSettlementKey={taskInput.acceptedUserMessageId}
          showAgentSelector={false}
          showIsolationSelector={false}
        />
      </div>
    </section>
  );
}

// Live App Server cards and their persisted Chat records have different message IDs.
// Permission request identity keeps the same DOM row mounted across that handoff.
export function chatRowKey(message: ChatMessage) {
  if (message.message.kind !== "permission") return message.message_id;
  const requestId = message.message.app_server_request_id ?? message.message.request_id;
  return `permission:${requestId}:${message.message.tool_call.id}`;
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
