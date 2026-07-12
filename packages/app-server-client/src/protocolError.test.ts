import { describe, expect, it } from "vitest";
import { AppServerProtocolError, errorEnvelopeFromUnknown, protocolErrorFromUnknown } from "./protocolError";

describe("protocol errors", () => {
  it("parses App Server error envelopes", () => {
    const envelope = errorEnvelopeFromUnknown({
      error: {
        code: "staleCursor",
        message: "Refresh required",
        recoverable: true,
        target: { method: "task/list", field: "cursor" },
      },
      meta: { clientRequestId: "client-request-1" },
    });

    expect(envelope).toEqual({
      error: {
        code: "staleCursor",
        message: "Refresh required",
        recoverable: true,
        target: { method: "task/list", field: "cursor" },
      },
      meta: { clientRequestId: "client-request-1" },
    });
  });

  it("wraps App Server error envelopes in Error instances", () => {
    const error = protocolErrorFromUnknown({
      error: {
        code: "validationFailed",
        message: "Invalid task",
        recoverable: false,
      },
    });

    expect(error).toBeInstanceOf(AppServerProtocolError);
    expect(error).toMatchObject({
      message: "Invalid task",
      protocolError: {
        code: "validationFailed",
        recoverable: false,
      },
    });
  });

  it("preserves authoritative Task state carried by a revision conflict", () => {
    const currentTask = { task: { taskId: "task-a" }, revision: 9 };

    const envelope = errorEnvelopeFromUnknown({
      error: {
        code: "conflict",
        message: "Task changed before the message was sent",
        recoverable: true,
        target: { field: "taskRevision", currentTask },
      },
    });

    expect(envelope?.error.target).toMatchObject({
      field: "taskRevision",
      currentTask,
    });
  });
});
