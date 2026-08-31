import { act, create } from "react-test-renderer";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type {
  AppServerEvent,
  AppServerSession,
  AppServerStateObserver,
  SubscriptionScope,
  SubscriptionSnapshot,
} from "@openaide/app-server-client";

import { useSubagentSessions } from "./useSubagentSessions";

type Observer = { onSnapshot(snapshot: SubscriptionSnapshot, event?: AppServerEvent): void };

describe("useSubagentSessions", () => {
  let observers: Array<{ scope: SubscriptionScope; observer: Observer }>;
  let latest: ReturnType<typeof useSubagentSessions>;
  let connection: Pick<AppServerSession, "request" | "subscribeState">;
  let request: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    observers = [];
    request = vi.fn();
    connection = {
      request,
      subscribeState(scope: SubscriptionScope, observer: AppServerStateObserver) {
        const entry = { scope, observer: observer as Observer };
        observers.push(entry);
        return () => {
          observers = observers.filter((candidate) => candidate !== entry);
        };
      },
    } as unknown as Pick<AppServerSession, "request" | "subscribeState">;
  });

  function Probe() {
    latest = useSubagentSessions({
      connection,
      enabled: true,
      taskId: "task_1",
    });
    return null;
  }

  it("keeps catalog and selected histories independently subscribed and marks unseen activity", async () => {
    act(() => { create(<Probe />); });
    const catalog = observers.find(({ scope }) => scope.kind === "subagentCatalog")!;

    act(() => catalog.observer.onSnapshot(catalogSnapshot(1, 0)));
    expect(latest.catalog?.entries[0]?.name).toBe("Explorer");

    act(() => latest.selectSubagent("subagent_11111111111111111111111111111111"));
    expect(observers.map(({ scope }) => scope.kind)).toEqual(["subagentCatalog", "subagentHistory"]);

    const history = observers.find(({ scope }) => scope.kind === "subagentHistory")!;
    act(() => history.observer.onSnapshot(historySnapshot()));
    expect(latest.history?.chat.items).toEqual([]);

    request.mockResolvedValue({
      taskId: "task_1",
      subagentId: "subagent_11111111111111111111111111111111",
      items: [{ messageId: "message_1", role: "agent", status: "complete", parts: [] }],
      hasBefore: false,
      totalCount: 1,
      revision: 1,
      startCursor: "cursor_1",
      endCursor: "cursor_1",
    });
    act(() => { latest.loadEarlier("cursor_2"); });
    expect(request).toHaveBeenCalledWith("task/chatPage", {
      taskId: "task_1",
      subagentId: "subagent_11111111111111111111111111111111",
      beforeCursor: "cursor_2",
      limit: 50,
    });
    await act(async () => { await Promise.resolve(); });
    expect(latest.history?.chat.items[0]?.messageId).toBe("message_1");

    act(() => latest.selectSubagent(undefined));
    act(() => catalog.observer.onSnapshot(catalogSnapshot(2, 1)));
    expect(latest.unseen.has("subagent_11111111111111111111111111111111")).toBe(true);
  });

  it("presents child text events through a terminal catalog update without animating the baseline", () => {
    act(() => { create(<Probe />); });
    const catalog = observers.find(({ scope }) => scope.kind === "subagentCatalog")!;
    act(() => catalog.observer.onSnapshot(catalogSnapshot(1, 1)));
    act(() => latest.selectSubagent("subagent_11111111111111111111111111111111"));
    const history = observers.find(({ scope }) => scope.kind === "subagentHistory")!;

    act(() => history.observer.onSnapshot(historySnapshot(1, "Forecast")));
    expect((latest as unknown as { liveTextPresentation?: unknown }).liveTextPresentation).toBeUndefined();

    act(() => {
      history.observer.onSnapshot(
        historySnapshot(2, "Forecast updated"),
        subagentHistoryEvent("event-child-text", historySnapshot(2, "Forecast updated")),
      );
      catalog.observer.onSnapshot(catalogSnapshot(2, 2, "completed"));
    });
    expect((latest as unknown as { liveTextPresentation?: { agent?: { eventCursor: string } } })
      .liveTextPresentation?.agent?.eventCursor).toBe(
        "event-child-text",
      );
  });
});

function catalogSnapshot(
  revision: number,
  historyRevision: number,
  status: "running" | "completed" = "running",
): SubscriptionSnapshot {
  return {
    kind: "subagentCatalog",
    catalog: {
      taskId: "task_1",
      revision,
      entries: [{
        subagentId: "subagent_11111111111111111111111111111111",
        name: "Explorer",
        delegatedTask: "Inspect the code",
        status,
        capabilities: { cancel: false, close: false },
        spawnedOrder: 1,
        historyRevision,
      }],
    },
  } as unknown as SubscriptionSnapshot;
}

function subagentHistoryEvent(cursor: string, snapshot: SubscriptionSnapshot): AppServerEvent {
  if (snapshot.kind !== "subagentHistory") throw new Error("expected subagent history");
  return {
    cursor,
    payload: { kind: "subagentHistoryUpdated", history: snapshot.history },
  } as unknown as AppServerEvent;
}

function historySnapshot(revision = 0, text?: string): SubscriptionSnapshot {
  return {
    kind: "subagentHistory",
    history: {
      taskId: "task_1",
      subagentId: "subagent_11111111111111111111111111111111",
      revision,
      availability: "available",
      chat: {
        items: text ? [{
          messageId: "child-agent-message",
          role: "agent",
          status: "streaming",
          parts: [{ kind: "text", text }],
        }] : [],
        hasMessages: Boolean(text),
      },
    },
  } as unknown as SubscriptionSnapshot;
}
