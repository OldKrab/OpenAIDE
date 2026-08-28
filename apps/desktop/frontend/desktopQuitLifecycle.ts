export type DesktopQuitDependencies = {
  requestDetach(): Promise<unknown>;
  closeSession(): Promise<void> | void;
  beforeExit?(outcome: DesktopQuitOutcome): void;
  exitApp(): Promise<unknown> | void;
};

export type DesktopQuitOutcome = "detached" | "detachFailed";

/** Gives the App Server a graceful last-client signal before the native process exits. */
export async function quitDesktop(
  dependencies: DesktopQuitDependencies,
): Promise<DesktopQuitOutcome> {
  let outcome: DesktopQuitOutcome = "detached";
  try {
    await dependencies.requestDetach();
  } catch {
    outcome = "detachFailed";
  }

  try {
    await dependencies.closeSession();
  } finally {
    dependencies.beforeExit?.(outcome);
    await dependencies.exitApp();
  }
  return outcome;
}
