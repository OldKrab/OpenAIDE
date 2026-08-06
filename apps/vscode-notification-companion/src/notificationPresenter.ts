export type NotificationPresenterEnvironment = {
  registerCommand(
    command: string,
    handler: (...args: unknown[]) => unknown,
  ): { dispose(): void };
  notify(message: string): Promise<void>;
  reportFailure(message: string): void;
  log(message: string, fields?: Record<string, unknown>): void;
};

const SHOW_SYSTEM_NOTIFICATION_COMMAND = "_openaide.notifications.show";
const TEST_SYSTEM_NOTIFICATION_COMMAND = "openaide.testSystemNotification";
const TEST_NOTIFICATION_MESSAGE = "OpenAIDE system notifications are working.";

/** Registers the UI-host command that presents remote Task events on the local desktop. */
export function registerNotificationPresenter(environment: NotificationPresenterEnvironment) {
  const showNotification = environment.registerCommand(SHOW_SYSTEM_NOTIFICATION_COMMAND, async (event) => {
    const message = notificationMessage(event);
    environment.log("presenting local OS notification");
    try {
      await environment.notify(message);
    } catch (error) {
      environment.log("local OS notification failed", {
        error: error instanceof Error ? error.message : String(error),
      });
      throw error;
    }
  });
  const testNotification = environment.registerCommand(TEST_SYSTEM_NOTIFICATION_COMMAND, async () => {
    try {
      await environment.notify(TEST_NOTIFICATION_MESSAGE);
      environment.log("local OS notification test succeeded");
    } catch (error) {
      environment.log("local OS notification test failed", {
        error: error instanceof Error ? error.message : String(error),
      });
      environment.reportFailure(
        "OpenAIDE could not show a system notification. Check the OpenAIDE Notifications output for details.",
      );
    }
  });
  return {
    dispose() {
      showNotification.dispose();
      testNotification.dispose();
    },
  };
}

function notificationMessage(event: unknown) {
  if (!event || typeof event !== "object" || !("message" in event)) {
    throw new Error("OpenAIDE notification event must include a message");
  }
  const message = event.message;
  if (typeof message !== "string" || message.trim().length === 0) {
    throw new Error("OpenAIDE notification message must be a non-empty string");
  }
  return message;
}
