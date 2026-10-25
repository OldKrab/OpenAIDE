import { describe, expect, it, vi } from "vitest";
import type { TaskSummary } from "@openaide/app-shell-contracts";
import type { ComputerSwitch, ComputerSwitchState } from "../../../packages/frontend/src/services/computerSwitch";
import type { WebTaskNotificationManager } from "../../../packages/frontend/src/shells/webTaskNotifications";
import { takeThisComputerTask, watchThisComputerAttention, type ThisComputerAttentionHost } from "./desktopThisComputerAttention";

describe("this computer's attention while a paired computer is in use", () => {
  it("starts from the watcher's latest list and follows its changes", async () => {
    const host = fakeHost({ snapshot: [task("t1")] });
    watchThisComputerAttention(host);
    await settled();

    host.emit([task("t1"), task("t2")]);

    expect(host.reconciled).toEqual([["thisComputer", ["t1"]], ["thisComputer", ["t1", "t2"]]]);
  });

  it("ignores a snapshot older than an event that already came", async () => {
    let answer: (tasks: TaskSummary[]) => void = () => undefined;
    const host = fakeHost({ snapshot: new Promise((resolve) => { answer = resolve; }) });
    watchThisComputerAttention(host);
    await settled();
    host.emit([task("t2")]);

    answer([task("t1")]);
    await settled();

    expect(host.reconciled).toEqual([["thisComputer", ["t2"]]]);
  });

  it("keeps its receipts apart from the window's but shares the on/off choice", () => {
    const host = fakeHost({ snapshot: null });
    watchThisComputerAttention(host);

    host.managerStorage?.setItem("openaide.desktopNotifications.handled", "[\"e1\"]");
    host.values.set("openaide.desktopNotifications.enabled", "false");

    expect(host.values.get("openaide.desktopNotifications.handled.thisComputer")).toBe("[\"e1\"]");
    expect(host.values.has("openaide.desktopNotifications.handled")).toBe(false);
    expect(host.managerStorage?.getItem("openaide.desktopNotifications.enabled")).toBe("false");
  });

  it("switches back to this computer on a click and opens the Task after the reload", () => {
    const host = fakeHost({ snapshot: null });
    watchThisComputerAttention(host);

    host.click("t7");

    expect(host.switcher.request).toHaveBeenCalledWith(undefined);
    expect(takeThisComputerTask(host.session, host.now())).toBe("t7");
    expect(takeThisComputerTask(host.session, host.now())).toBeUndefined();
  });

  it("forgets the click when the switch did not happen", () => {
    const host = fakeHost({ snapshot: null });
    watchThisComputerAttention(host);
    host.click("t7");

    host.switcher.publish({ confirming: "", cancellable: false });
    expect(host.sessionValues.size).toBe(1);
    host.switcher.publish({ cancellable: false });

    expect(takeThisComputerTask(host.session, host.now())).toBeUndefined();
  });

  it("does not open a Task a long-past click asked for", () => {
    const host = fakeHost({ snapshot: null });
    watchThisComputerAttention(host);
    host.click("t7");

    // timing: data — a click's age compared as a value, never waited on.
    expect(takeThisComputerTask(host.session, host.now() + 11 * 60_000)).toBeUndefined();
  });
});

function fakeHost({ snapshot }: { snapshot: TaskSummary[] | null | Promise<TaskSummary[]> }) {
  const values = new Map<string, string>();
  const sessionValues = new Map<string, string>();
  const reconciled: Array<[string, string[]]> = [];
  let emitter: (tasks: TaskSummary[]) => void = () => undefined;
  let clicker: (taskId: string) => void = () => undefined;
  let state: ComputerSwitchState = { cancellable: false };
  const listeners = new Set<() => void>();
  const switcher = {
    snapshot: () => state,
    subscribe(listener: () => void) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    request: vi.fn(async () => { state = { pending: "", cancellable: false }; }),
    confirm: async () => undefined,
    decline: () => undefined,
    cancel: () => undefined,
    dismiss: () => undefined,
    publish(next: ComputerSwitchState) {
      state = next;
      for (const listener of [...listeners]) listener();
    },
  } satisfies ComputerSwitch & { publish(next: ComputerSwitchState): void };
  const storageOf = (map: Map<string, string>) => ({
    getItem: (key: string) => map.get(key) ?? null,
    setItem: (key: string, value: string) => { map.set(key, value); },
    removeItem: (key: string) => { map.delete(key); },
  });
  const host: ThisComputerAttentionHost & {
    values: typeof values;
    sessionValues: typeof sessionValues;
    reconciled: typeof reconciled;
    managerStorage?: ThisComputerAttentionHost["storage"];
    switcher: typeof switcher;
    emit(tasks: TaskSummary[]): void;
    click(taskId: string): void;
  } = {
    values,
    sessionValues,
    reconciled,
    notifications(storage, openTask) {
      host.managerStorage = storage;
      clicker = openTask;
      return { reconcile: (root: string, tasks: TaskSummary[]) => { reconciled.push([root, tasks.map((item) => item.task_id)]); } } as unknown as WebTaskNotificationManager;
    },
    storage: storageOf(values),
    session: storageOf(sessionValues),
    snapshot: () => Promise.resolve(snapshot),
    listen: async (handler) => { emitter = handler; },
    switcher,
    now: () => Date.parse("2026-10-23T09:00:00.000Z"),
    emit: (tasks) => emitter(tasks),
    click: (taskId) => clicker(taskId),
  };
  return host;
}

function task(taskId: string): TaskSummary {
  return { task_id: taskId, title: "Fix the build" } as TaskSummary;
}

async function settled() {
  for (let index = 0; index < 5; index += 1) await Promise.resolve();
}
