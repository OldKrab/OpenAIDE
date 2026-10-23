/** The delays before each retry of a first initialize that keeps failing. */
export const INITIALIZE_RETRY_DELAYS_MS = [2_000, 5_000, 15_000, 30_000, 60_000] as const;

export type AppServerInitializeRetry = {
  /** Records a failed attempt and schedules the next one. */
  failed(): void;
  /** Runs a scheduled retry at once; false when none is waiting. */
  retryNow(): boolean;
  dispose(): void;
};

/**
 * Retries the controller's first `client/initialize` until it succeeds.
 *
 * A first initialize that fails leaves no session for the connection to
 * recover, yet what made it fail often passes on its own: App Server is still
 * starting, the paired computer is out of reach, or it is older than this
 * Frontend until it is updated. The retry backs off, and it runs at once when
 * the app returns to the screen or the network comes back, which is when the
 * user looks again.
 */
export function createAppServerInitializeRetry(options: {
  attempt: () => void;
  subscribeToResume: (resumed: () => void) => () => void;
  delaysMs?: readonly number[];
}): AppServerInitializeRetry {
  const delays = options.delaysMs ?? INITIALIZE_RETRY_DELAYS_MS;
  let failures = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let disposed = false;
  const stopResume = options.subscribeToResume(() => {
    retryNow();
  });

  function retryNow() {
    if (disposed || timer === undefined) return false;
    clearTimeout(timer);
    timer = undefined;
    options.attempt();
    return true;
  }

  return {
    failed() {
      if (disposed) return;
      if (timer !== undefined) clearTimeout(timer);
      failures += 1;
      timer = setTimeout(retryNow, delays[Math.min(failures, delays.length) - 1]);
    },
    retryNow,
    dispose() {
      disposed = true;
      if (timer !== undefined) clearTimeout(timer);
      timer = undefined;
      stopResume();
    },
  };
}
