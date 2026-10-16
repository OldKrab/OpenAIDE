import type { ActionPerformed, Options } from "@tauri-apps/plugin-notification";
import {
  createWebTaskNotificationManager,
  type WebTaskNotificationEnvironment,
  type WebTaskNotificationManager,
} from "../../../packages/frontend/src/shells/webTaskNotifications";

/** The native seams Desktop notifications use; injected so tests need no Tauri runtime. */
export type DesktopNotificationHost = {
  storage: WebTaskNotificationEnvironment["storage"];
  isPermissionGranted(): Promise<boolean>;
  requestPermission(): Promise<NotificationPermission>;
  sendNotification(options: Options): void;
  removeActive(notifications: Array<{ id: number }>): Promise<void>;
  onAction(listener: (event: ActionPerformed) => void): Promise<unknown>;
  initiallyFocused(): boolean;
  isFocused(): Promise<boolean>;
  onFocusChanged(listener: (focused: boolean) => void): Promise<unknown>;
  focusedTaskId(): string | undefined;
  focusWindow(): Promise<void>;
  openTask(taskId: string): void;
  now(): number;
};

/**
 * Presents Task Attention Events as OS notifications through the Tauri plugin.
 *
 * The shared manager keeps owning receipts, focus-at-occurrence, and one current
 * notification per Task; Desktop only supplies native permission, focus, and
 * presentation. Desktop has a single window, so cross-tab coordination is a no-op,
 * and only an event of the Task that window shows while focused stays silent.
 * A click arrives through the plugin's `onAction` with the shown notification's
 * payload: its identifier finds the live click handler, and its Task identifier
 * still routes a click whose handler is gone.
 */
export function createDesktopTaskNotifications(host: DesktopNotificationHost): WebTaskNotificationManager {
  const focusListeners = new Set<() => void>();
  const clickHandlers = new Map<number, () => void>();
  let focused = host.initiallyFocused();
  let permission: NotificationPermission = "default";
  let nextId = 1;
  const changed = () => { for (const listener of focusListeners) listener(); };

  // Desktop permission needs no prompt, so a default-on choice obtains it right away.
  void host.isPermissionGranted()
    .then((granted) => granted ? "granted" : host.requestPermission())
    .then(
      (state) => { permission = state; changed(); },
      () => { console.warn("desktop_notification_permission_read_failed error_kind=plugin"); },
    );
  void host.isFocused().then((value) => { focused = value; changed(); }, () => undefined);
  void host.onFocusChanged((value) => { focused = value; changed(); });
  // Without a listener the plugin reports no clicks, so it must exist before the first notification.
  void host.onAction((event) => {
    if (event.actionId !== "tap") return;
    const id = event.notification?.id;
    const handler = id === undefined ? undefined : clickHandlers.get(id);
    console.info(`desktop_notification_activated notification_id=${id ?? "unknown"} handler=${handler ? "live" : "missing"}`);
    if (handler) { handler(); return; }
    const taskId = event.notification?.extra?.taskId;
    if (typeof taskId === "string") {
      host.openTask(taskId);
      void host.focusWindow().catch(() => console.warn("desktop_notification_focus_failed error_kind=window"));
    }
  }).catch(() => console.warn("desktop_notification_action_listener_failed error_kind=plugin"));

  return createWebTaskNotificationManager({
    storage: host.storage,
    // The plugin shows the application icon; a renderer asset URL is not a native icon.
    notificationIconUrl: "",
    now: host.now,
    isFocused: () => focused,
    focusedTaskId: host.focusedTaskId,
    notificationsSupported: () => true,
    enabledByDefault: true,
    notificationPermission: () => permission,
    async requestNotificationPermission() {
      try {
        permission = await host.requestPermission();
      } catch {
        console.warn("desktop_notification_permission_request_failed error_kind=plugin");
        permission = "denied";
      }
      return permission;
    },
    showNotification(title, options, onClick) {
      const id = nextId++;
      const taskId = options.tag?.replace(/^openaide-task-/, "");
      try {
        host.sendNotification({ id, title, body: options.body, extra: taskId ? { taskId } : undefined });
        console.info(`desktop_notification_shown notification_id=${id}`);
      } catch {
        console.warn(`desktop_notification_failed notification_id=${id} error_kind=plugin`);
        return { close: () => undefined };
      }
      clickHandlers.set(id, onClick);
      return {
        close() {
          if (!clickHandlers.delete(id)) return;
          // Only Linux and the BSDs can withdraw a shown notification; elsewhere it stays until dismissed.
          void host.removeActive([{ id }]).catch(() => undefined);
        },
      };
    },
    focusWindow: () => {
      void host.focusWindow().catch(() => console.warn("desktop_notification_focus_failed error_kind=window"));
    },
    openTask: (taskId) => host.openTask(taskId),
    subscribeFocus(listener) {
      focusListeners.add(listener);
      return () => focusListeners.delete(listener);
    },
    publish: () => undefined,
    subscribeMessages: () => () => undefined,
  });
}
