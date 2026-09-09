import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { BackendConnection, FileViewerSnapshot } from "@openaide/app-server-client";
import {
  openFileViewer,
  openFileViewerFromHandle,
  refreshFileViewer,
  releaseFileViewer,
} from "../intents/fileViewerIntents";

type FileViewerConnection = Pick<BackendConnection, "request">;
export type FileViewerPendingTab = {
  handle: string;
  displayPath: string;
  basename: string;
  kind: "pending";
  truncated: false;
};
export type FileViewerTab = FileViewerSnapshot | FileViewerPendingTab;

/** One ephemeral tab/capability owner per Task and transport. Late completions are
 * released, never inserted into a different Task or a tab the user already closed. */
export function useTaskFileViewer({
  connection: input,
  enabled,
  taskId,
  workspaceRoot,
}: {
  connection?: FileViewerConnection;
  enabled: boolean;
  taskId: string;
  workspaceRoot?: string;
}) {
  const request = input?.request;
  const owner = useMemo(
    () => ({
      connection: request ? { request } : undefined,
      handles: new Set<string>(),
      pending: new Map<string, string>(),
      requests: new Map<string, number>(),
      sequence: 0,
    }),
    [request, taskId, workspaceRoot, enabled],
  );
  const currentOwner = useRef(owner);
  currentOwner.current = owner;
  const [tabs, setTabs] = useState<FileViewerTab[]>([]);
  const tabsRef = useRef(tabs);
  const [activeHandle, setActiveHandle] = useState<string>();
  const activeRef = useRef(activeHandle);
  activeRef.current = activeHandle;
  const [collapsed, setCollapsed] = useState(false);
  const [splitRatio, setSplitRatio] = useState(0.45);
  const update = useCallback((next: FileViewerTab[]) => {
    tabsRef.current = next;
    setTabs(next);
  }, []);
  function release(handle: string) {
    if (owner.connection && !handle.startsWith("pending:")) {
      owner.handles.delete(handle);
      void releaseFileViewer(owner.connection, handle).catch(() => undefined);
    }
  }
  useEffect(() => {
    update([]);
    setActiveHandle(undefined);
    setCollapsed(false);
    return () => {
      owner.pending.clear();
      owner.requests.clear();
      for (const handle of owner.handles)
        if (owner.connection) void releaseFileViewer(owner.connection, handle).catch(() => undefined);
      owner.handles.clear();
    };
  }, [owner, update]);
  const settle = useCallback(
    (snapshot: FileViewerTab, previous: string, activate: boolean) => {
      const existing = tabsRef.current.find(
        (tab) => tab.handle !== previous && tab.displayPath === snapshot.displayPath,
      );
      if (existing && existing.handle !== snapshot.handle) release(existing.handle);
      let found = false;
      const next = tabsRef.current
        .filter((tab) => tab.handle !== existing?.handle)
        .map((tab) => {
          if (tab.handle !== previous) return tab;
          found = true;
          return snapshot;
        });
      if (!found) next.push(snapshot);
      if (snapshot.kind !== "pending" && !snapshot.handle.startsWith("pending:")) owner.handles.add(snapshot.handle);
      update(next);
      if (activate || activeRef.current === previous) {
        setActiveHandle(snapshot.handle);
        activeRef.current = snapshot.handle;
      }
      setCollapsed(false);
    },
    [owner, update],
  );
  const openPath = useCallback(
    async (path: string, line?: number) => {
      if (!enabled || !owner.connection || !path.trim()) return;
      const key = path.trim();
      const pendingHandle = owner.pending.get(key);
      if (pendingHandle) {
        setActiveHandle(pendingHandle);
        return;
      }
      const sequence = ++owner.sequence;
      const pending: FileViewerPendingTab = {
        handle: `pending:${crypto.randomUUID()}`,
        displayPath: key,
        basename: key.split(/[/\\]/).at(-1) ?? key,
        kind: "pending",
        truncated: false,
      };
      owner.pending.set(key, pending.handle);
      settle(pending, "", true);
      try {
        const snapshot = await openFileViewer(owner.connection, taskId, key, line);
        if (currentOwner.current !== owner || owner.pending.get(key) !== pending.handle) {
          void releaseFileViewer(owner.connection, snapshot.handle).catch(() => undefined);
          return;
        }
        settle(snapshot, pending.handle, sequence === owner.sequence);
      } catch {
        if (currentOwner.current === owner && owner.pending.get(key) === pending.handle)
          settle(
            {
              handle: pending.handle as FileViewerSnapshot["handle"],
              displayPath: key,
              basename: pending.basename,
              kind: "error",
              error: "unreadable",
              truncated: false,
            },
            pending.handle,
            sequence === owner.sequence,
          );
      } finally {
        if (owner.pending.get(key) === pending.handle) owner.pending.delete(key);
      }
    },
    [enabled, owner, settle, taskId],
  );
  const openFromHandle = useCallback(
    async (handle: string, href: string) => {
      if (!enabled || !owner.connection) return;
      const sequence = ++owner.sequence;
      try {
        const snapshot = await openFileViewerFromHandle(owner.connection, handle, href);
        if (currentOwner.current !== owner || !owner.handles.has(handle)) {
          void releaseFileViewer(owner.connection, snapshot.handle).catch(() => undefined);
          return;
        }
        settle(snapshot, "", sequence === owner.sequence);
      } catch {
        /* The originating tab remains available for retry. */
      }
    },
    [enabled, owner, settle],
  );
  const refresh = useCallback(
    async (handle: string, line?: number) => {
      if (!enabled || !owner.connection) return;
      const tab = tabsRef.current.find((item) => item.handle === handle);
      if (!tab) return;
      if (handle.startsWith("pending:")) {
        await openPath(tab.displayPath, line);
        return;
      }
      const sequence = (owner.requests.get(handle) ?? 0) + 1;
      owner.requests.set(handle, sequence);
      try {
        const snapshot = await refreshFileViewer(owner.connection, handle, line);
        if (currentOwner.current !== owner || !owner.handles.has(handle) || owner.requests.get(handle) !== sequence)
          return;
        update(tabsRef.current.map((item) => (item.handle === handle ? snapshot : item)));
      } catch {
        /* Existing snapshot and Refresh remain available. */
      }
    },
    [enabled, owner, openPath, update],
  );
  const closeTab = useCallback(
    (handle: string) => {
      for (const [key, pending] of owner.pending) if (pending === handle) owner.pending.delete(key);
      owner.requests.delete(handle);
      const next = tabsRef.current.filter((tab) => tab.handle !== handle);
      update(next);
      setActiveHandle((active) => (active === handle ? next.at(-1)?.handle : active));
      release(handle);
    },
    [owner, update],
  );
  const focusTab = (handle: string, line?: number) =>
    update(
      tabsRef.current.map((tab) =>
        tab.handle === handle && tab.kind !== "pending" ? { ...tab, focusLine: line } : tab,
      ),
    );
  const activeTab = tabs.find((tab) => tab.handle === activeHandle) ?? tabs.at(-1);
  return {
    activeTab,
    focusTab,
    collapsed,
    closeTab,
    openFromHandle,
    openPath,
    refresh,
    setCollapsed,
    setSplitRatio,
    splitRatio,
    tabs,
    selectTab: (handle: string) => {
      owner.sequence++;
      setActiveHandle(handle);
      activeRef.current = handle;
    },
    visible: enabled && tabs.length > 0,
  };
}
