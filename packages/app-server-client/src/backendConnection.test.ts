import { describe, expect, it } from "vitest";
import { TASK_CANCEL, type TaskId } from "./generated/protocol";
import { backendRequest } from "./backendConnection";

describe("BackendConnection", () => {
  it("builds typed request records with generated method params", () => {
    const request = backendRequest(TASK_CANCEL, { taskId: "task-1" as TaskId });

    expect(request).toEqual({
      method: "task/cancel",
      params: { taskId: "task-1" },
    });
  });
});
