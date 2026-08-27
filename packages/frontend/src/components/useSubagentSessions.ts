import { useEffect, useMemo, useRef, useState } from "react";
import type {
  AppServerSession,
  SubagentCatalogSnapshot,
  SubagentHistorySnapshot,
  SubagentId,
  TaskId,
} from "@openaide/app-server-client";

type SubagentConnection = Pick<AppServerSession, "subscribeState">;

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
  const [selectedSubagentId, setSelectedSubagentId] = useState<string>();
  const [unseen, setUnseen] = useState<Set<string>>(() => new Set());
  const revisions = useRef(new Map<string, number>());
  const selectedRef = useRef<string | undefined>(undefined);
  selectedRef.current = selectedSubagentId;

  useEffect(() => {
    setCatalog(undefined);
    setHistory(undefined);
    setSelectedSubagentId(undefined);
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
        onSnapshot(snapshot) {
          if (snapshot.kind === "subagentHistory") setHistory(snapshot.history);
        },
      },
    );
  }, [connection, enabled, selectedSubagentId, taskId]);

  const selected = useMemo(
    () => catalog?.entries.find((entry) => entry.subagentId === selectedSubagentId),
    [catalog, selectedSubagentId],
  );

  return {
    catalog,
    history: history?.subagentId === selectedSubagentId ? history : undefined,
    selected,
    selectedSubagentId,
    selectSubagent: setSelectedSubagentId,
    unseen,
  };
}
