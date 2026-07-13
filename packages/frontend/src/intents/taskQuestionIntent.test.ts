import { describe, expect, it, vi } from "vitest";
import type { BackendConnection } from "@openaide/app-server-client";
import { respondToQuestionIntent } from "./taskIntents";
import { createInitialState } from "../state/store";

describe("respondToQuestionIntent", () => {
  it("responds through the typed Question server-request method", async () => {
    const dispatch = vi.fn();
    const respond = vi.fn(() => Promise.resolve());

    respondToQuestionIntent(
      { backendConnection: { respond: respond as BackendConnection["respond"] }, dispatch, state: createInitialState() },
      "question-1",
      { action: "submit", content: { scope: "form", count: 3 } },
    );
    await Promise.resolve();

    expect(respond).toHaveBeenCalledWith("question-1", {
      action: "submit",
      content: { scope: "form", count: 3 },
    });
    expect(dispatch).toHaveBeenNthCalledWith(1, { type: "question:responding", requestId: "question-1" });
    expect(dispatch).toHaveBeenCalledTimes(1);
  });

  it("keeps a rejected response recoverable", async () => {
    const dispatch = vi.fn();
    const respond = vi.fn(() => Promise.reject(new Error("Already answered")));

    respondToQuestionIntent(
      { backendConnection: { respond: respond as BackendConnection["respond"] }, dispatch, state: createInitialState() },
      "question-1",
      { action: "cancel" },
    );
    await Promise.resolve();

    expect(dispatch).toHaveBeenLastCalledWith({
      type: "question:error",
      requestId: "question-1",
      message: "Already answered",
    });
  });
});
