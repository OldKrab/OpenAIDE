import { memo, useCallback, useEffect, useState } from "react";
import { ArrowDown, Check, CircleAlert } from "lucide-react";
import type {
  ActivityStep,
  ChatMessage,
  ElicitationResponse,
  TaskSnapshot,
} from "@openaide/app-shell-contracts";
import type { ToolImagePreview } from "@openaide/app-server-client";
import { renderedChat } from "../state/chatPaging";
import type { AppState, TaskLiveTextPresentation } from "../state/store";
import { ChatContentSizeChangeContext } from "./ChatActivityView";
import { ChatRow } from "./ChatMessageView";
import { QuoteSelectionAction } from "./QuoteSelectionAction";
import {
  permissionResponseForMessage,
  questionResponseForMessage,
} from "./taskChatPresentation";
import { timestampMillis } from "./taskSurfaceHelpers";
import { useTaskChatScroll } from "./useTaskChatScroll";
import { UserMessageNavigator } from "./UserMessageNavigator";

export type TaskChatTimelineRow =
  | { key: "archived"; kind: "archived" }
  | { key: "load-earlier"; kind: "loadEarlier" }
  | { key: "chat-error"; kind: "error" }
  | {
      key: string;
      kind: "message";
      message: ChatMessage;
      permissionQueueCount?: number;
      permissionTool?: Extract<ActivityStep, { kind: "tool" }>;
    }
  | { key: "timeline-status"; kind: "status" };

type TaskChatTimelineProps = {
  archived: boolean;
  canRestoreTask: boolean;
  chat: ReturnType<typeof renderedChat>;
  chatScroll: ReturnType<typeof useTaskChatScroll>;
  commandCatalog: TaskSnapshot["agent_commands"];
  items: ChatMessage[];
  liveTextPresentation?: TaskLiveTextPresentation;
  onLoadChatPage: (beforeCursor: string) => number | undefined;
  onLoadToolImagePreview?: (artifactId: string) => Promise<ToolImagePreview | undefined>;
  onOpenSubagent?: (subagentId: string) => void;
  onPermissionRespond: (requestId: string, optionId: string) => void;
  onQuestionRespond: (requestId: string, response: ElicitationResponse) => void;
  onQuote?: (text: string) => void;
  onRestoreTask: (taskId: string) => void;
  onSubscribeToolDetail: (artifactId: string) => () => void;
  permissionResponses: AppState["permissionResponses"];
  questionResponses: AppState["questionResponses"];
  rows: TaskChatTimelineRow[];
  taskId: string;
  taskStatus: TaskSnapshot["task"]["status"];
  toolDetails: AppState["toolDetails"];
  timelineStatusKind: "blocked" | "notice" | "progress";
  timelineStatusLabel?: string;
  workingStartedAt?: string;
};

/** Renders only the measured Chat rows selected by the virtualizer. */
export const TaskChatTimeline = memo(function TaskChatTimeline({
  archived,
  canRestoreTask,
  chat,
  chatScroll,
  commandCatalog,
  items,
  liveTextPresentation,
  onLoadChatPage,
  onLoadToolImagePreview,
  onOpenSubagent,
  onPermissionRespond,
  onQuestionRespond,
  onQuote,
  onRestoreTask,
  onSubscribeToolDetail,
  permissionResponses,
  questionResponses,
  rows,
  taskId,
  taskStatus,
  toolDetails,
  timelineStatusKind,
  timelineStatusLabel,
  workingStartedAt,
}: TaskChatTimelineProps) {
  const [messageListElement, setMessageListElement] = useState<HTMLDivElement | null>(null);
  const setMessageListRef = useCallback((element: HTMLDivElement | null) => {
    chatScroll.messageListRef.current = element;
    setMessageListElement(element);
  }, [chatScroll.messageListRef]);
  const latestTextMessageIds = latestTextMessageIdsByChannel(items);
  const virtualItems = chatScroll.virtualizer.getVirtualItems();
  const measureChangedChatContent = useCallback((content: HTMLElement) => {
    const row = content.closest<HTMLElement>(".message-list-virtual-row");
    if (!row) return;
    const index = Number(row.dataset.index);
    if (!Number.isInteger(index)) return;
    // Disclosure state commits before paint. Measure here too so the next
    // absolute row moves in the same frame instead of awaiting ResizeObserver.
    chatScroll.virtualizer.resizeItem(index, row.offsetHeight);
  }, [chatScroll.virtualizer]);
  return (
    <div className="message-list-shell" data-more-below={String(chatScroll.moreBelow)}>
      <div
        className="message-list"
        onKeyDown={chatScroll.onKeyDown}
        onPointerCancel={chatScroll.onPointerCancel}
        onPointerDown={chatScroll.onPointerDown}
        onPointerUp={chatScroll.onPointerUp}
        onScroll={chatScroll.onScroll}
        onWheel={chatScroll.onWheel}
        ref={setMessageListRef}
        >
        <ChatContentSizeChangeContext.Provider value={measureChangedChatContent}>
          <div className="message-list-virtualizer" ref={chatScroll.virtualizer.containerRef}>
            {virtualItems.map((virtualRow) => {
              const row = rows[virtualRow.index];
              if (!row) return null;
              const previousRow = rows[virtualRow.index - 1];
              return (
              <div
                className="message-list-virtual-row"
                data-after-activity={String(
                  previousRow?.kind === "message" && previousRow.message.message.kind === "activity",
                )}
                data-index={virtualRow.index}
                data-row-key={row.key}
                data-row-kind={row.kind}
                data-user-message-navigation-target={
                  row.kind === "message"
                  && row.message.message.kind === "user"
                  && chatScroll.userMessageNavigation.targetKey === row.key
                    ? "true"
                    : undefined
                }
                key={virtualRow.key}
                ref={chatScroll.virtualizer.measureElement}
                style={{ position: "absolute", top: 0, left: 0 }}
              >
                {row.kind === "archived" ? (
                  <div className="archived-task-notice" role="status">
                    <span>Archived task. Restore it to send a follow-up.</span>
                    {canRestoreTask ? (
                      <button type="button" onClick={() => onRestoreTask(taskId)}>
                        Restore
                      </button>
                    ) : null}
                  </div>
                ) : row.kind === "loadEarlier" ? (
                  <div className="load-earlier-row">
                    <button
                      disabled={chat.pending || !chat.beforeCursor}
                      onClick={() => {
                        if (!chat.beforeCursor || chat.pending) return;
                        chatScroll.loadEarlier(chat.beforeCursor);
                      }}
                      type="button"
                    >
                      {chat.pending ? "Loading earlier" : "Load earlier"}
                    </button>
                  </div>
                ) : row.kind === "error" ? (
                  <p className="chat-system">{chat.error}</p>
                ) : row.kind === "message" ? (
                  <ChatRow
                    commandCatalog={commandCatalog}
                    liveTextEventCursor={liveTextCursorForMessage(
                      liveTextPresentation,
                      latestTextMessageIds,
                      row.message,
                    )}
                    message={row.message}
                    onLoadToolImagePreview={onLoadToolImagePreview}
                    onOpenSubagent={onOpenSubagent}
                    onPermissionRespond={onPermissionRespond}
                    onQuestionRespond={onQuestionRespond}
                    onSubscribeToolDetail={onSubscribeToolDetail}
                    permissionQueueCount={row.permissionQueueCount}
                    permissionResponse={permissionResponseForMessage(row.message.message, permissionResponses)}
                    permissionTool={row.permissionTool}
                    presentLiveText={
                      isLiveTextMessage(liveTextPresentation, row.message)
                    }
                    questionResponse={questionResponseForMessage(row.message.message, questionResponses)}
                    taskId={taskId}
                    toolDetails={toolDetails}
                  />
                ) : timelineStatusLabel ? (
                  <TimelineStatus
                    kind={timelineStatusKind}
                    label={timelineStatusLabel}
                    startedAt={workingStartedAt}
                  />
                ) : null}
              </div>
              );
            })}
          </div>
        </ChatContentSizeChangeContext.Provider>
      </div>
      <UserMessageNavigator navigation={chatScroll.userMessageNavigation} />
      {onQuote && messageListElement ? (
        <QuoteSelectionAction key={taskId} onQuote={onQuote} root={messageListElement} />
      ) : null}
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

/** Builds stable row identities so prepends can anchor to the same visible message. */
export function buildTaskChatTimelineRows({
  archived,
  chat,
  items,
  timelineStatusLabel,
}: {
  archived: boolean;
  chat: ReturnType<typeof renderedChat>;
  items: ChatMessage[];
  timelineStatusLabel?: string;
}): TaskChatTimelineRow[] {
  const toolSteps = permissionToolSteps(items);
  const messageRows: Extract<TaskChatTimelineRow, { kind: "message" }>[] = [];
  const permissionRowsByTool = new Map<string, number>();
  for (const message of items) {
    const permission = message.message.kind === "permission" && message.message.state === "pending"
      ? message.message
      : undefined;
    const existingIndex = permission ? permissionRowsByTool.get(permission.tool_call.id) : undefined;
    if (existingIndex !== undefined) {
      const existing = messageRows[existingIndex];
      existing.permissionQueueCount = (existing.permissionQueueCount ?? 0) + 1;
      continue;
    }
    const row: Extract<TaskChatTimelineRow, { kind: "message" }> = {
      key: `message:${chatRowKey(message)}`,
      kind: "message",
      message,
      permissionTool: permission ? toolSteps.get(permission.tool_call.id) : undefined,
    };
    if (permission) permissionRowsByTool.set(permission.tool_call.id, messageRows.length);
    messageRows.push(row);
  }
  return [
    ...(archived ? [{ key: "archived", kind: "archived" } as const] : []),
    ...(chat.hasBefore ? [{ key: "load-earlier", kind: "loadEarlier" } as const] : []),
    ...(chat.error ? [{ key: "chat-error", kind: "error" } as const] : []),
    ...messageRows,
    ...(timelineStatusLabel ? [{ key: "timeline-status", kind: "status" } as const] : []),
  ];
}

function permissionToolSteps(items: ChatMessage[]) {
  const tools = new Map<string, Extract<ActivityStep, { kind: "tool" }>>();
  for (const item of items) {
    if (item.message.kind !== "activity") continue;
    for (const step of item.message.steps) {
      if (step.kind === "tool" && step.tool_call_id) tools.set(step.tool_call_id, step);
    }
  }
  return tools;
}

export function chatRowKey(message: ChatMessage) {
  return message.message_id;
}

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

export function isLiveTextMessage(
  presentation: TaskLiveTextPresentation | undefined,
  message: ChatMessage,
) {
  if (message.message.kind !== "agent_message") return false;
  return presentation?.[message.message.role]?.messageId === message.message_id;
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
