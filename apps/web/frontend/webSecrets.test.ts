import { describe, expect, it, vi } from "vitest";
import type { AppShellSecretStore } from "@openaide/app-shell-contracts";
import { createWebSecretMessageHandler } from "./webSecrets";

function memoryStore(): AppShellSecretStore & { values: Map<string, string> } {
  const values = new Map<string, string>();
  return {
    values,
    async delete(key) { values.delete(key); },
    async get(key) { return values.get(key); },
    async store(key, value) { values.set(key, value); },
  };
}

describe("Web secret messages", () => {
  it("applies a transaction and answers typed secret reads without echoing the value", async () => {
    const store = memoryStore();
    const log = vi.fn();
    const handle = createWebSecretMessageHandler(store, log);
    const applied = await handle({
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

    expect(applied).toEqual({
      type: "secret.transaction.result",
      payload: { requestId: "apply-1", transactionId: "transaction-1", ok: true },
    });
    await expect(handle({
      type: "appServer.serverRequest",
      payload: {
        requestId: "read-1",
        method: "secret/read",
        params: { key: "openaide.agent.codex.auth.key.env.OPENAI_API_KEY" },
      },
    })).resolves.toEqual({
      type: "appServer.serverRequest.result",
      payload: {
        requestId: "read-1",
        method: "secret/read",
        result: { value: "secret-value" },
      },
    });
    expect(JSON.stringify(log.mock.calls)).not.toContain("secret-value");
  });

  it("logs a rejected transaction as a safe terminal failure", async () => {
    const store = memoryStore();
    store.store = async () => { throw new Error("sensitive provider detail"); };
    const log = vi.fn();
    const handle = createWebSecretMessageHandler(store, log);

    await expect(handle({
      type: "secret.transaction.apply",
      payload: {
        requestId: "apply-failure",
        transactionId: "transaction-failure",
        changes: {
          writes: [{
            target: { kind: "agentEnvironment", agentId: "codex", name: "OPENAI_API_KEY" },
            value: "secret-value",
          }],
          deletes: [],
        },
      },
    })).resolves.toMatchObject({ payload: { ok: false } });

    expect(log).toHaveBeenLastCalledWith("secret_transaction_apply_completed", {
      correlation_id: "transaction-failure",
      duration_ms: expect.any(Number),
      error_kind: "secure_storage",
      outcome: "failure",
    });
    expect(JSON.stringify(log.mock.calls)).not.toContain("secret-value");
    expect(JSON.stringify(log.mock.calls)).not.toContain("sensitive provider detail");
  });
});
