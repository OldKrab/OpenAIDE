export type DesktopQuitDependencies = {
  requestDetach(): Promise<unknown>;
  closeSession(): Promise<void> | void;
  beforeExit?(outcome: DesktopQuitOutcome): void;
  exitApp(): Promise<unknown> | void;
};

export type DesktopQuitOutcome = "detached" | "detachFailed" | "detachTimeout";

// Explicit Quit must always reach `exitApp`. The App Server detach is a local
// handshake, so a hung connection is a failure to report, not a reason to keep
// the native app resident. These budgets stay inside the native menu watchdog.
const DETACH_TIMEOUT_MS = 3_000;
const CLOSE_TIMEOUT_MS = 2_000;

type BoundedResult = "completed" | "failed" | "timedOut";

/** Gives the App Server a graceful last-client signal before the native process exits. */
export async function quitDesktop(
  dependencies: DesktopQuitDependencies,
): Promise<DesktopQuitOutcome> {
  let outcome: DesktopQuitOutcome = "detached";
  const detach = await runBounded(dependencies.requestDetach, DETACH_TIMEOUT_MS);
  if (detach === "failed") outcome = "detachFailed";
  else if (detach === "timedOut") outcome = "detachTimeout";

  // The session can only be closed while this process is still alive; the native
  // app must exit even when closing the connection neither settles nor fails.
  await runBounded(dependencies.closeSession, CLOSE_TIMEOUT_MS);
  dependencies.beforeExit?.(outcome);
  await dependencies.exitApp();
  return outcome;
}

async function runBounded(
  operation: () => Promise<unknown> | void,
  timeoutMs: number,
): Promise<BoundedResult> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      Promise.resolve().then(operation).then(
        () => "completed" as BoundedResult,
        () => "failed" as BoundedResult,
      ),
      new Promise<BoundedResult>((resolve) => {
        timer = setTimeout(() => resolve("timedOut"), timeoutMs);
      }),
    ]);
  } finally {
    if (timer !== undefined) clearTimeout(timer);
  }
}
