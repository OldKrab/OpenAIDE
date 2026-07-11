import { describe, expect, it } from "vitest";
import type { ChatMessage, ElicitationMessage } from "@openaide/app-shell-contracts";
import { chatItemsWithAppServerQuestions, questionResponseForMessage } from "../components/taskChatPresentation";
import { appReducer } from "./appReducer";
import { createInitialState } from "./store";

describe("Question runtime state", () => {
  it("tracks concurrent response attempts independently", () => {
    let state = createInitialState();
    state = appReducer(state, { type: "question:responding", requestId: "question-1" });
    state = appReducer(state, { type: "question:responding", requestId: "question-2" });
    state = appReducer(state, { type: "question:error", requestId: "question-1", message: "Try again." });

    expect(state.questionResponses).toEqual({
      "question-1": { responding: false, error: "Try again." },
      "question-2": { responding: true },
    });
    expect(questionResponseForMessage(question("question-2").message, state.questionResponses)).toEqual({ responding: true });
  });

  it("shows live Questions only in their bound Task and stops appending after snapshot ingestion", () => {
    const live = question("question-1");
    const requests = { "question-1": { taskId: "task-1", message: live } };

    expect(chatItemsWithAppServerQuestions([], requests, "task-2")).toEqual([]);
    expect(chatItemsWithAppServerQuestions([], requests, "task-1")).toEqual([live]);
    expect(chatItemsWithAppServerQuestions([live], requests, "task-1")).toEqual([live]);
  });

  it("stores live Questions separately from permissions", () => {
    const live = question("question-1");
    const state = appReducer(createInitialState(), {
      type: "appServerQuestion:received",
      requestId: "question-1",
      taskId: "task-1",
      message: live,
    });

    expect(state.appServerQuestionRequests["question-1"]).toEqual({ taskId: "task-1", message: live });
    expect(state.appServerPermissionRequests).toEqual({});
  });
});

function question(requestId: string): ChatMessage & { message: ElicitationMessage } {
  return {
    cursor: requestId,
    identity: requestId,
    message_id: requestId,
    message_type: "elicitation",
    message: {
      kind: "elicitation",
      id: requestId,
      request_id: requestId,
      app_server_request_id: requestId,
      prompt: "Choose a scope.",
      state: "pending",
      created_at: "2026-07-10T00:00:00Z",
      fields: [],
    },
  };
}
