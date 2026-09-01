import { describe, expect, it } from "vitest";

import { SHELL_OPEN_EXTERNAL } from "./generated/protocol.js";
import { parseLocalHttpWireMessages } from "./localHttpWire.js";

describe("parseLocalHttpWireMessages", () => {
  it("accepts a shell request to open an external authentication URL", () => {
    const messages = parseLocalHttpWireMessages(
      JSON.stringify({
        jsonrpc: "2.0",
        id: "auth-url-1",
        method: SHELL_OPEN_EXTERNAL,
        scope: { kind: "task", taskId: "task-1" },
        params: { url: "https://auth.openai.com/device" },
      }),
    );

    expect(messages).toEqual([
      {
        kind: "serverRequest",
        request: {
          requestId: "auth-url-1",
          method: SHELL_OPEN_EXTERNAL,
          scope: { kind: "task", taskId: "task-1" },
          params: { url: "https://auth.openai.com/device" },
        },
      },
    ]);
  });
});
