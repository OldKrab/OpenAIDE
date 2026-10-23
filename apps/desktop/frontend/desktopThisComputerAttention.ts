import type { TaskSummary } from "@openaide/app-shell-contracts";
import type { ComputerSwitch } from "../../../packages/frontend/src/services/computerSwitch";
import {
  TASK_NOTIFICATION_RECEIPTS_KEY,
  type WebTaskNotificationEnvironment,
  type WebTaskNotificationManager,
} from "../../../packages/frontend/src/shells/webTaskNotifications";

type Storage = WebTaskNotificationEnvironment["storage"];
type SessionStorage = Pick<globalThis.Storage, "getItem" | "setItem" | "removeItem">;

/** The native watcher's event and its latest list (`desktop_local_attention.rs`). */
export const THIS_COMPUTER_ATTENTION_EVENT = "desktop-local-attention";
export const THIS_COMPUTER_ATTENTION_SNAPSHOT = "desktop_local_attention_snapshot";
/** The Task a notification click asked for, carried across the switch's page reload. */
const OPEN_TASK_KEY = "openaide.desktopThisComputerTask";
/** A click whose switch never finished must not open a Task on some later switch. */
const OPEN_TASK_TTL_MS = 10 * 60_000;
const STATE_ROOT = "thisComputer";

export type ThisComputerAttentionHost = {
  /** Creates the manager for this computer's Tasks on `storage`; its clicks call `openTask`. */
  notifications(storage: Storage, openTask: (taskId: string) => void): WebTaskNotificationManager;
  storage: Storage;
  session: SessionStorage;
  snapshot(): Promise<TaskSummary[] | null>;
  listen(handler: (tasks: TaskSummary[]) => void): Promise<unknown>;
  switcher: ComputerSwitch;
  now(): number;
};

/**
 * Notifies this computer's Tasks while the window uses a paired computer
 * (ADR-0070 Attention). The native watcher supplies the attention list; a
 * second notification manager applies the usual rules to it with its own
 * receipts, sharing the user's on/off choice with the window's manager. A click
 * switches back to this computer through the shared switch, so unsent input is
 * still asked about, and the reloaded page opens the Task.
 */
export function watchThisComputerAttention(host: ThisComputerAttentionHost): WebTaskNotificationManager {
  const manager = host.notifications(receiptsApart(host.storage), (taskId) => {
    try {
      host.session.setItem(OPEN_TASK_KEY, JSON.stringify({ taskId, at: host.now() }));
    } catch {
      console.warn("desktop_this_computer_attention_open_failed error_kind=storage");
    }
    console.info("desktop_this_computer_attention_open outcome=switching");
    // The page stays only if the switch did not happen; then the click is spent.
    const stop = host.switcher.subscribe(() => {
      const state = host.switcher.snapshot();
      if (state.pending !== undefined || state.confirming !== undefined) return;
      stop();
      host.session.removeItem(OPEN_TASK_KEY);
      console.info("desktop_this_computer_attention_open outcome=stayed");
    });
    void host.switcher.request(undefined);
  });
  let heard = false;
  void host.listen((tasks) => {
    heard = true;
    manager.reconcile(STATE_ROOT, tasks);
  }).catch(() => console.warn("desktop_this_computer_attention_listen_failed error_kind=event"));
  // A reloaded window starts from the watcher's latest list, unless an event already came.
  void host.snapshot().then(
    (tasks) => {
      console.info(`desktop_this_computer_attention_snapshot outcome=${tasks ? "found" : "empty"} tasks=${tasks?.length ?? 0}`);
      if (tasks && !heard) manager.reconcile(STATE_ROOT, tasks);
    },
    () => console.warn("desktop_this_computer_attention_snapshot outcome=failure error_kind=command"),
  );
  return manager;
}

/** The Task a notification click asked to open on this computer, taken once. */
export function takeThisComputerTask(session: SessionStorage, now: number): string | undefined {
  let stored: { taskId?: unknown; at?: unknown } | undefined;
  try {
    const value = session.getItem(OPEN_TASK_KEY);
    session.removeItem(OPEN_TASK_KEY);
    stored = value ? JSON.parse(value) as typeof stored : undefined;
  } catch {
    return undefined;
  }
  if (typeof stored?.taskId !== "string" || typeof stored.at !== "number") return undefined;
  const fresh = now - stored.at <= OPEN_TASK_TTL_MS;
  console.info(`desktop_this_computer_attention_open outcome=${fresh ? "arrived" : "expired"}`);
  return fresh ? stored.taskId : undefined;
}

/** Shares every setting with the window's manager but keeps this computer's receipts apart. */
function receiptsApart(storage: Storage): Storage {
  const own = (key: string) => key === TASK_NOTIFICATION_RECEIPTS_KEY ? `${key}.thisComputer` : key;
  return {
    getItem: (key) => storage.getItem(own(key)),
    setItem: (key, value) => storage.setItem(own(key), value),
    removeItem: (key) => storage.removeItem(own(key)),
  };
}
