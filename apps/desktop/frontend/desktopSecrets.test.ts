import { describe, expect, it, vi } from "vitest";
import { createDesktopSecretMessageHandler } from "./desktopSecrets";

describe("Desktop secret messages", () => {
  it("uses only fixed native credential commands for transactions and reads", async () => {
    const values = new Map<string, string>();
    const invoke = vi.fn(async (command: string, args?: Record<string, unknown>) => {
      const key = args?.key as string;
      if (command === "desktop_secret_read") return values.get(key) ?? null;
      if (command === "desktop_secret_write") { values.set(key, args?.value as string); return; }
      if (command === "desktop_secret_delete") { values.delete(key); return; }
      throw new Error("unexpected command");
    });
    const handle = createDesktopSecretMessageHandler(invoke);

    await handle({
      type: "secret.transaction.apply",
      payload: {
        requestId: "apply-1",
        transactionId: "transaction-1",
        changes: {
          writes: [{
            target: { kind: "agentEnvironment", agentId: "codex.auth.key", name: "OPENAI_API_KEY" },
            value: "secret-value",
          }],
          deletes: [],
        },
      },
    });
    const result = await handle({
      type: "appServer.serverRequest",
      payload: {
        requestId: "read-1",
        method: "secret/read",
        params: { key: "openaide.agent.codex.auth.key.env.OPENAI_API_KEY" },
      },
    });

    expect(result).toEqual({
      type: "appServer.serverRequest.result",
      payload: { requestId: "read-1", method: "secret/read", result: { value: "secret-value" } },
    });
    expect(invoke).toHaveBeenCalledWith("desktop_secret_write", {
      key: "openaide.agent.codex.auth.key.env.OPENAI_API_KEY",
      value: "secret-value",
    });
  });

  it("answers a native credential read failure without making the App Server wait", async () => {
    const invoke = vi.fn(async () => { throw new Error("native provider detail"); });
    const handle = createDesktopSecretMessageHandler(invoke);

    await expect(handle({
      type: "appServer.serverRequest",
      payload: {
        requestId: "read-failure",
        method: "secret/read",
        params: { key: "openaide.agent.codex.env.OPENAI_API_KEY" },
      },
    })).resolves.toEqual({
      type: "appServer.serverRequest.result",
      payload: { requestId: "read-failure", method: "secret/read", result: { value: null } },
    });
  });
});
