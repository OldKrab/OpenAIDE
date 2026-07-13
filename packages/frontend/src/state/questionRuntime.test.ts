import { describe, expect, it } from "vitest";
import { appReducer } from "./appReducer";
import { createInitialState } from "./store";

describe("Question response state", () => {
  it("tracks concurrent response attempts independently", () => {
    let state = createInitialState();
    state = appReducer(state, { type: "question:responding", requestId: "question-1" });
    state = appReducer(state, { type: "question:responding", requestId: "question-2" });
    state = appReducer(state, { type: "question:error", requestId: "question-1", message: "Try again." });

    expect(state.questionResponses).toEqual({
      "question-1": { responding: false, error: "Try again." },
      "question-2": { responding: true },
    });
  });
});
