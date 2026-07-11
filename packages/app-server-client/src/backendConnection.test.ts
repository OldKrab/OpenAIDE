import { describe, expect, it } from "vitest";
import {
  PERMISSION_REQUEST,
  TASK_CANCEL,
  type RequestId,
  type ServerRequestMethod,
  type ServerRequestResponseResultByMethod,
  type TaskId,
} from "./generated/protocol";
import { backendRequest, type BackendConnection } from "./backendConnection";

describe("BackendConnection", () => {
  it("builds typed request records with generated method params", () => {
    const request = backendRequest(TASK_CANCEL, { taskId: "task-1" as TaskId });

    expect(request).toEqual({
      method: "task/cancel",
      params: { taskId: "task-1" },
    });
  });

  it("keeps server-request responses on the generic respond channel", async () => {
    const responses: Array<{
      requestId: string;
      result: ServerRequestResponseResultByMethod[ServerRequestMethod];
    }> = [];
    const connection: Pick<BackendConnection, "respond"> = {
      respond(requestId, result) {
        responses.push({ requestId, result });
      },
    };

    await connection.respond<typeof PERMISSION_REQUEST>("server-request-1" as RequestId, {
      optionId: "allow-once",
    });

    expect(responses).toEqual([
      {
        requestId: "server-request-1",
        result: { optionId: "allow-once" },
      },
    ]);
  });
});
