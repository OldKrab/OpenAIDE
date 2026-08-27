import { act, create } from "react-test-renderer";
import { beforeEach, describe, expect, it } from "vitest";
import type { AppServerSession, SubscriptionScope, SubscriptionSnapshot } from "@openaide/app-server-client";

import { useSubagentSessions } from "./useSubagentSessions";

type Observer = { onSnapshot(snapshot: SubscriptionSnapshot): void };

describe("useSubagentSessions", () => {
  let observers: Array<{ scope: SubscriptionScope; observer: Observer }>;
  let latest: ReturnType<typeof useSubagentSessions>;
  let connection: Pick<AppServerSession, "subscribeState">;

  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    observers = [];
    connection = {
      subscribeState(scope, observer) {
        const entry = { scope, observer: observer as Observer };
        observers.push(entry);
        return () => {
          observers = observers.filter((candidate) => candidate !== entry);
        };
      },
    } as Pick<AppServerSession, "subscribeState">;
  });

  function Probe() {
    latest = useSubagentSessions({
      connection,
      enabled: true,
      taskId: "task_1",
    });
    return null;
  }

  it("keeps catalog and selected histories independently subscribed and marks unseen activity", () => {
    act(() => { create(<Probe />); });
    const catalog = observers.find(({ scope }) => scope.kind === "subagentCatalog")!;

    act(() => catalog.observer.onSnapshot(catalogSnapshot(1, 0)));
    expect(latest.catalog?.entries[0]?.name).toBe("Explorer");

    act(() => latest.selectSubagent("subagent_11111111111111111111111111111111"));
    expect(observers.map(({ scope }) => scope.kind)).toEqual(["subagentCatalog", "subagentHistory"]);

    const history = observers.find(({ scope }) => scope.kind === "subagentHistory")!;
    act(() => history.observer.onSnapshot(historySnapshot()));
    expect(latest.history?.chat.items).toEqual([]);

    act(() => latest.selectSubagent(undefined));
    act(() => catalog.observer.onSnapshot(catalogSnapshot(2, 1)));
    expect(latest.unseen.has("subagent_11111111111111111111111111111111")).toBe(true);
  });
});

function catalogSnapshot(revision: number, historyRevision: number): SubscriptionSnapshot {
  return {
    kind: "subagentCatalog",
    catalog: {
      taskId: "task_1",
      revision,
      entries: [{
        subagentId: "subagent_11111111111111111111111111111111",
        name: "Explorer",
        delegatedTask: "Inspect the code",
        status: "running",
        capabilities: { cancel: false, close: false },
        spawnedOrder: 1,
        historyRevision,
      }],
    },
  } as unknown as SubscriptionSnapshot;
}

function historySnapshot(): SubscriptionSnapshot {
  return {
    kind: "subagentHistory",
    history: {
      taskId: "task_1",
      subagentId: "subagent_11111111111111111111111111111111",
      revision: 0,
      availability: "available",
      chat: { items: [], hasMessages: false },
    },
  } as unknown as SubscriptionSnapshot;
}
