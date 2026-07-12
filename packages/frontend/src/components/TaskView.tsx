import { useCallback, useEffect, useState } from "react";
import type { Dispatch } from "react";
import { ArrowDown } from "lucide-react";
import type { AppPreferencesRecord, ConfigOptionsCatalog, ElicitationResponse, PermissionDecision, TaskSnapshot, TaskSummary } from "@openaide/app-shell-contracts";
import type { AppAction } from "../state/appReducer";
import { renderedChat } from "../state/chatPaging";
import type { AppState, TaskComposerInput } from "../state/store";
import { ChatRow } from "./ChatMessageView";
import { Composer } from "./Composer";
import { TaskHeader } from "./TaskHeader";
import {
  chatFollowModeForPosition,
  initialTaskScrollTop,
  scrollTopAfterPrependedContent,
  scrollTopForGeneratedContent,
  taskComposerAvailability,
} from "./TaskViewModel";
import { taskWorkingStatusLabel, workspaceLabel } from "./taskSurfaceHelpers";
import type { TaskFileBrowserCallbacks } from "./appControllerCallbackTypes";
import {
  chatItemsWithAppServerPermissions,
  chatItemsWithAppServerQuestions,
  chatItemsWithPendingInput,
  permissionResponseForMessage,
  questionResponseForMessage,
  taskChatHasLiveUpdates,
} from "./taskChatPresentation";
import { useTaskChatScroll } from "./useTaskChatScroll";
import { appServerAttachmentHandles } from "../state/composerOptions";

export {
  chatFollowModeForPosition,
  initialTaskScrollTop,
  scrollTopAfterPrependedContent,
  scrollTopForGeneratedContent,
  taskComposerAvailability,
} from "./TaskViewModel";
export {
  chatItemsWithAppServerPermissions,
  chatItemsWithAppServerQuestions,
  chatItemsWithPendingInput,
  permissionResponseForMessage,
  questionResponseForMessage,
} from "./taskChatPresentation";

export function TaskLoadingView({ error }: { error?: string }) {
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
      {error ? <p className="inline-error">{error}</p> : null}
    </section>
  );
}

export function TaskView({
  activeTask,
  appServerPermissionRequests,
  appServerQuestionRequests = {},
  archived = false,
  backendReady,
  chatPageState,
  dispatch,
  onCancel,
  fileBrowser,
  onLoadChatPage,
  onLoadToolDetail,
  onPermissionRespond,
  onQuestionRespond,
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
  savedScrollTop,
  taskInput,
  toolDetails,
  submitShortcut,
  showWorkspaceContext = true,
}: {
  activeTask?: TaskSummary;
  appServerPermissionRequests: AppState["appServerPermissionRequests"];
  appServerQuestionRequests?: AppState["appServerQuestionRequests"];
  archived?: boolean;
  backendReady: boolean;
  chatPageState: AppState["chatPages"][string] | undefined;
  dispatch: Dispatch<AppAction>;
  onCancel: () => void;
  fileBrowser?: TaskFileBrowserCallbacks;
  onLoadChatPage: (beforeCursor: string) => void;
  onLoadToolDetail: (artifactId: string, refresh?: boolean) => void;
  onPermissionRespond: (
    requestId: string,
    optionId: string,
    decision: PermissionDecision,
    source?: "agent" | "appServer",
  ) => void;
  onQuestionRespond?: (requestId: string, response: ElicitationResponse) => void;
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
  savedScrollTop?: number;
  taskInput: TaskComposerInput;
  toolDetails: AppState["toolDetails"];
  submitShortcut: AppPreferencesRecord["composer_submit_shortcut"];
  showWorkspaceContext?: boolean;
}) {
  const inputPending = taskInput.pending !== undefined;
  const chat = renderedChat(snapshot, chatPageState);
  const chatItems = chatItemsWithAppServerQuestions(
    chatItemsWithAppServerPermissions(
      chatItemsWithPendingInput(chat.items, taskInput, snapshot.task.task_id),
      appServerPermissionRequests,
      snapshot.task.task_id,
    ),
    appServerQuestionRequests,
    snapshot.task.task_id,
  );
  const preparationBlocked = chatItems.some((item) => item.message_id === "app-server-preparation");
  const turnBusy = snapshot.task.status === "active";
  const composerAvailability = taskComposerAvailability({
    archived,
    backendReady,
    inputPending,
    preparationBlocked,
    taskStatus: snapshot.task.status,
  });
  const composerDisabled = composerAvailability.editingDisabled;
  const attachmentsSendable = taskInput.context.length === 0
    || appServerAttachmentHandles(taskInput.context) !== undefined;
  const canSend = !composerAvailability.sendDisabled && attachmentsSendable;
  const [showHistoryUpdated, setShowHistoryUpdated] = useState(false);
  useEffect(() => {
    if (snapshot.history_sync.state !== "updated") {
      setShowHistoryUpdated(false);
      return undefined;
    }
    setShowHistoryUpdated(true);
    const timer = window.setTimeout(() => setShowHistoryUpdated(false), 2_000);
    return () => window.clearTimeout(timer);
  }, [snapshot.history_sync.state]);
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
  const recordTaskScroll = useCallback((scrollTop: number) => {
    dispatch({ type: "taskScroll:record", taskId: snapshot.task.task_id, scrollTop });
  }, [dispatch, snapshot.task.task_id]);
  const diagnosticItemKindCounts = chatItems.reduce<Record<string, number>>((counts, item) => {
    counts[item.message.kind] = (counts[item.message.kind] ?? 0) + 1;
    return counts;
  }, {});
  const chatScroll = useTaskChatScroll({
    diagnosticContext: {
      chatVersion: snapshot.chat.version,
      historySyncState: snapshot.history_sync.state,
      itemCount: chatItems.length,
      itemKindCounts: diagnosticItemKindCounts,
      olderItemCount: chatPageState?.olderItems.length ?? 0,
      pendingPermissions: chatItems
        .filter((item) => item.message.kind === "permission" && item.message.state === "pending")
        .map((item) => item.message.kind === "permission"
          ? item.message.app_server_request_id ?? item.message.request_id
          : item.message_id),
      snapshotRevision: snapshot.revision,
      taskStatus: snapshot.task.status,
    },
    generating: taskChatHasLiveUpdates({
      inputPending,
      taskStatus: snapshot.task.status,
    }),
    historySyncState: snapshot.history_sync.state,
    itemCount: chat.items.length,
    onScrollTop: recordTaskScroll,
    pendingPrepend: chat.pending,
    savedScrollTop,
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
                  chatScroll.capturePrependAnchor();
                  onLoadChatPage(chat.beforeCursor);
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
              key={message.message_id}
              message={message}
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
        <Composer
          agentLocked
          attachments={taskInput.context}
          autoFocus
          configOptions={startupConfigOptions ?? snapshot.agent_config}
          commandCatalog={snapshot.agent_commands}
          disabled={composerDisabled}
          error={taskInput.error}
          fileBrowser={fileBrowser}
          focusRequestKey={snapshot.task.task_id}
          onCancel={
            turnBusy || inputPending
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
          submitPending={inputPending}
          submitPendingLabel="Sending message"
          submitRequiresText={!snapshot.send_capability.attachment_only}
          submissionSettlementKey={snapshot.revision}
          showAgentSelector={false}
          showIsolationSelector={false}
        />
      </div>
    </section>
  );
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
