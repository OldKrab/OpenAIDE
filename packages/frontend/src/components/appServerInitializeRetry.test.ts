import { afterEach, describe, expect, it, vi } from "vitest";
import { createAppServerInitializeRetry } from "./appServerInitializeRetry";

describe("createAppServerInitializeRetry", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  function retryWith(attempt: () => void) {
    let resume: (() => void) | undefined;
    const stopResume = vi.fn();
    const retry = createAppServerInitializeRetry({
      attempt,
      delaysMs: [10, 100],
      subscribeToResume: (resumed) => {
        resume = resumed;
        return stopResume;
      },
    });
    return { retry, resume: () => resume?.(), stopResume };
  }

  it("backs off between failed attempts and holds the longest delay", () => {
    vi.useFakeTimers();
    const attempt = vi.fn();
    const { retry } = retryWith(attempt);

    retry.failed();
    vi.advanceTimersByTime(9);
    expect(attempt).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(attempt).toHaveBeenCalledTimes(1);

    retry.failed();
    vi.advanceTimersByTime(99);
    expect(attempt).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(1);
    expect(attempt).toHaveBeenCalledTimes(2);

    retry.failed();
    vi.advanceTimersByTime(100);
    expect(attempt).toHaveBeenCalledTimes(3);
  });

  it("runs a waiting retry at once on resume, and nothing when none waits", () => {
    vi.useFakeTimers();
    const attempt = vi.fn();
    const { retry, resume } = retryWith(attempt);

    resume();
    expect(retry.retryNow()).toBe(false);
    expect(attempt).not.toHaveBeenCalled();

    retry.failed();
    resume();
    expect(attempt).toHaveBeenCalledTimes(1);
    // The timer it replaced does not run a second attempt.
    vi.advanceTimersByTime(1_000);
    expect(attempt).toHaveBeenCalledTimes(1);
  });

  it("stops when disposed", () => {
    vi.useFakeTimers();
    const attempt = vi.fn();
    const { retry, resume, stopResume } = retryWith(attempt);

    retry.failed();
    retry.dispose();
    resume();
    vi.advanceTimersByTime(1_000);
    retry.failed();
    vi.advanceTimersByTime(1_000);

    expect(attempt).not.toHaveBeenCalled();
    expect(retry.retryNow()).toBe(false);
    expect(stopResume).toHaveBeenCalledOnce();
  });
});
