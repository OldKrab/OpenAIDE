import { useEffect, useMemo, useRef, useState } from "react";
import type {
  AppServerSession,
  ChatItem,
  MessageId,
  SubagentCatalogSnapshot,
  SubagentHistorySnapshot,
  SubagentId,
  TaskId,
} from "@openaide/app-server-client";
import { TASK_CHAT_PAGE } from "@openaide/app-server-client";
import type { TaskLiveTextPresentation } from "../state/store";

type SubagentConnection = Pick<AppServerSession, "request" | "subscribeState">;

/** Owns only ephemeral inspection selection and subscription replicas. Durable
 * identity, hierarchy, status, and history remain App Server-owned. */
export function useSubagentSessions({
  connection,
  enabled,
  taskId,
}: {
  connection?: SubagentConnection;
  enabled: boolean;
  taskId: string;
}) {
  const [catalog, setCatalog] = useState<SubagentCatalogSnapshot>();
  const [history, setHistory] = useState<SubagentHistorySnapshot>();
  const [olderItems, setOlderItems] = useState<ChatItem[]>([]);
  const [olderHasBefore, setOlderHasBefore] = useState<boolean>();
  const [olderStartCursor, setOlderStartCursor] = useState<MessageId>();
  const [selectedSubagentId, setSelectedSubagentId] = useState<string>();
  const [liveTextPresentation, setLiveTextPresentation] = useState<TaskLiveTextPresentation>();
  const [unseen, setUnseen] = useState<Set<string>>(() => new Set());
  const revisions = useRef(new Map<string, number>());
  const selectedRef = useRef<string | undefined>(undefined);
  const pageRequest = useRef(0);
  selectedRef.current = selectedSubagentId;

  useEffect(() => {
    setCatalog(undefined);
    setHistory(undefined);
    setOlderItems([]);
    setOlderHasBefore(undefined);
    setOlderStartCursor(undefined);
    setSelectedSubagentId(undefined);
    setLiveTextPresentation(undefined);
    setUnseen(new Set());
    revisions.current = new Map();
  }, [taskId]);

  useEffect(() => {
    if (!connection || !enabled) return;
    return connection.subscribeState(
      { kind: "subagentCatalog", taskId: taskId as TaskId },
      {
        onSnapshot(snapshot) {
          if (snapshot.kind !== "subagentCatalog") return;
          const next = snapshot.catalog;
          setUnseen((current) => {
            const updated = new Set(current);
            for (const entry of next.entries) {
              const prior = revisions.current.get(entry.subagentId);
              if (prior !== undefined && entry.historyRevision > prior && entry.subagentId !== selectedRef.current) {
                updated.add(entry.subagentId);
              }
              revisions.current.set(entry.subagentId, entry.historyRevision);
            }
            return updated;
          });
          setCatalog(next);
        },
      },
    );
  }, [connection, enabled, taskId]);

  useEffect(() => {
    if (!connection || !enabled || !selectedSubagentId) {
      setHistory(undefined);
      return;
    }
    setHistory(undefined);
    setOlderItems([]);
    setOlderHasBefore(undefined);
    setOlderStartCursor(undefined);
    setLiveTextPresentation(undefined);
    pageRequest.current += 1;
    setUnseen((current) => {
      const updated = new Set(current);
      updated.delete(selectedSubagentId);
      return updated;
    });
    return connection.subscribeState(
      {
        kind: "subagentHistory",
        taskId: taskId as TaskId,
        subagentId: selectedSubagentId as SubagentId,
      },
      {
        onSnapshot(snapshot, event) {
          if (snapshot.kind !== "subagentHistory") return;
          setHistory(snapshot.history);
          if (event?.payload.kind !== "subagentHistoryUpdated") return;
          const presentation = latestTextPresentation(
            snapshot.history.chat.items,
            event.cursor,
          );
          if (presentation) setLiveTextPresentation(presentation);
        },
      },
    );
  }, [connection, enabled, selectedSubagentId, taskId]);

  const selected = useMemo(
    () => catalog?.entries.find((entry) => entry.subagentId === selectedSubagentId),
    [catalog, selectedSubagentId],
  );
  const visibleHistory = useMemo(() => {
    if (!history || history.subagentId !== selectedSubagentId || olderItems.length === 0) {
      return history?.subagentId === selectedSubagentId ? history : undefined;
    }
    const seen = new Set<string>();
    const items = [...olderItems, ...history.chat.items].filter((item) => {
      if (seen.has(item.messageId)) return false;
      seen.add(item.messageId);
      return true;
    });
    return {
      ...history,
      chat: {
        ...history.chat,
        items,
        hasMoreBefore: olderHasBefore,
        startCursor: olderStartCursor,
      },
      startCursor: olderStartCursor,
    };
  }, [history, olderHasBefore, olderItems, olderStartCursor, selectedSubagentId]);

  const loadEarlier = (beforeCursor: string) => {
    if (!connection || !selectedSubagentId) return undefined;
    const requestId = ++pageRequest.current;
    const requestedSubagentId = selectedSubagentId;
    void connection.request(TASK_CHAT_PAGE, {
      taskId: taskId as TaskId,
      subagentId: requestedSubagentId as SubagentId,
      beforeCursor: beforeCursor as MessageId,
      limit: 50,
    }).then((page) => {
      if (selectedRef.current !== requestedSubagentId || pageRequest.current !== requestId) return;
      setOlderItems((current) => [...page.items, ...current]);
      setOlderHasBefore(page.hasBefore);
      setOlderStartCursor(page.startCursor ?? undefined);
    }).catch(() => undefined);
    return requestId;
  };

  return {
    catalog,
    history: visibleHistory,
    liveTextPresentation,
    loadEarlier,
    selected,
    selectedSubagentId,
    selectSubagent: setSelectedSubagentId,
    unseen,
  };
}

function latestTextPresentation(items: ChatItem[], eventCursor: string): TaskLiveTextPresentation | undefined {
  const presentation: TaskLiveTextPresentation = {};
  for (let index = items.length - 1; index >= 0; index -= 1) {
    const item = items[index];
    if (!item || (item.role !== "agent" && item.role !== "system")) continue;
    if (!item.parts.some((part) => part.kind === "text")) continue;
    const channel = item.role === "system" ? "thought" : "agent";
    presentation[channel] ??= { messageId: item.messageId, eventCursor };
    if (presentation.agent && presentation.thought) break;
  }
  return presentation.agent || presentation.thought ? presentation : undefined;
}
