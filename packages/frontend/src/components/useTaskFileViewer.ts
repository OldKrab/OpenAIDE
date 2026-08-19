import { useCallback, useEffect, useRef, useState } from "react";
import type { BackendConnection, FileViewerHandleId, FileViewerSnapshot } from "@openaide/app-server-client";
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

export function useTaskFileViewer({
  connection,
  enabled,
  taskId,
}: {
  connection?: FileViewerConnection;
  enabled: boolean;
  taskId: string;
}) {
  const [tabs, setTabs] = useState<FileViewerTab[]>([]);
  const [activeHandle, setActiveHandle] = useState<string>();
  const [collapsed, setCollapsed] = useState(false);
  const [splitRatio, setSplitRatio] = useState(0.45);
  const handlesRef = useRef<string[]>([]);
  const openingRef = useRef(new Set<string>());

  useEffect(() => {
    handlesRef.current = tabs
      .filter((tab) => tab.kind !== "pending")
      .map((tab) => tab.handle);
  }, [tabs]);

  const settleTab = useCallback((snapshot: FileViewerTab, fromHandle?: string) => {
    setTabs((current) => {
      const remaining = fromHandle
        ? current.filter((tab) => tab.handle !== fromHandle)
        : current;
      const existing = remaining.findIndex((tab) =>
        tab.displayPath === snapshot.displayPath
        || (
          snapshot.kind !== "pending"
          && tab.kind === "pending"
          && tab.basename === snapshot.basename
        )
      );
      const previous = existing >= 0 ? remaining[existing] : undefined;
      if (
        previous
        && previous.kind !== "pending"
        && snapshot.kind !== "pending"
        && previous.handle !== snapshot.handle
        && connection
      ) {
        void releaseFileViewer(connection, previous.handle);
      }
      if (existing >= 0) {
        const next = [...remaining];
        next[existing] = snapshot;
        return next;
      }
      return [...remaining, snapshot];
    });
    setActiveHandle(snapshot.handle);
    setCollapsed(false);
  }, [connection]);

  const openPath = useCallback(async (path: string, line?: number) => {
    if (!enabled || !connection) return;
    const key = path.trim();
    if (!key || openingRef.current.has(key)) return;
    openingRef.current.add(key);
    const pending = pendingTab(key);
    settleTab(pending);
    try {
      settleTab(await openFileViewer(connection, taskId, key, line), pending.handle);
    } catch {
      settleTab({
        handle: pending.handle as FileViewerHandleId,
        displayPath: pending.displayPath,
        basename: pending.basename,
        kind: "error",
        error: "unreadable",
        truncated: false,
      }, pending.handle);
    } finally {
      openingRef.current.delete(key);
    }
  }, [connection, enabled, settleTab, taskId]);

  const openFromHandle = useCallback(async (handle: string, href: string) => {
    if (!enabled || !connection) return;
    try {
      settleTab(await openFileViewerFromHandle(connection, handle, href));
    } catch {
      // Relative File Tab navigation failed; the originating tab remains.
    }
  }, [connection, enabled, settleTab]);

  const tabsRef = useRef<FileViewerTab[]>([]);
  tabsRef.current = tabs;

  const refresh = useCallback(async (handle: string, line?: number) => {
    if (!enabled || !connection) return;
    const current = tabsRef.current.find((tab) => tab.handle === handle);
    if (current && isPendingHandle(handle)) {
      await openPath(current.displayPath);
      return;
    }
    try {
      const snapshot = await refreshFileViewer(connection, handle, line);
      setTabs((tabs) => tabs.map((tab) => (tab.handle === handle ? snapshot : tab)));
      setActiveHandle(snapshot.handle);
    } catch {
      // Retry remains available on the existing File Tab.
    }
  }, [connection, enabled, openPath]);

  const closeTab = useCallback((handle: string) => {
    setTabs((current) => {
      const next = current.filter((tab) => tab.handle !== handle);
      setActiveHandle((active) => {
        if (active !== handle) return active;
        return next.at(-1)?.handle;
      });
      return next;
    });
    if (connection && !isPendingHandle(handle)) void releaseFileViewer(connection, handle);
  }, [connection]);

  useEffect(() => {
    setTabs([]);
    setActiveHandle(undefined);
    setCollapsed(false);
  }, [taskId]);

  useEffect(() => {
    return () => {
      if (!connection) return;
      for (const handle of handlesRef.current) void releaseFileViewer(connection, handle);
    };
  }, [connection, taskId]);

  const activeTab = tabs.find((tab) => tab.handle === activeHandle) ?? tabs.at(-1);

  return {
    activeTab,
    collapsed,
    closeTab,
    openFromHandle,
    openPath,
    refresh,
    setCollapsed,
    setSplitRatio,
    splitRatio,
    tabs,
    selectTab: setActiveHandle,
    visible: enabled && tabs.length > 0,
  };
}

function pendingTab(path: string): FileViewerPendingTab {
  const trimmed = path.trim();
  const basename = trimmed.split(/[/\\]/).pop()?.replace(/:([1-9]\d*)$/, "") || trimmed;
  return {
    handle: `pending:${crypto.randomUUID()}`,
    displayPath: trimmed,
    basename,
    kind: "pending",
    truncated: false,
  };
}

function isPendingHandle(handle: string) {
  return handle.startsWith("pending:");
}
