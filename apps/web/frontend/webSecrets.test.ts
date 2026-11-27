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

  it("lists the held key ids, never values, and follows saves, deletes and rollbacks", async () => {
    const handle = createWebSecretMessageHandler(memoryStore());
    const ref = { kind: "agentEnvironment", agentId: "codex", name: "OPENAI_API_KEY" } as const;
    const keys = async () => {
      const result = await handle({ type: "secret.keys.request", payload: { requestId: "keys" } });
      return result?.type === "secret.keys.result" ? result.payload.keyIds : undefined;
    };
    expect(await keys()).toEqual([]);

    await handle({
      type: "secret.transaction.apply",
      payload: { requestId: "a", transactionId: "t1", changes: { writes: [{ target: ref, value: "secret-value" }], deletes: [] } },
    });
    await handle({ type: "secret.transaction.commit", payload: { requestId: "c", transactionId: "t1" } });
    expect(await keys()).toEqual(["openaide.agent.codex.env.OPENAI_API_KEY"]);

    await handle({
      type: "secret.transaction.apply",
      payload: { requestId: "a2", transactionId: "t2", changes: { writes: [], deletes: [ref] } },
    });
    expect(await keys()).toEqual([]);
    await handle({ type: "secret.transaction.rollback", payload: { requestId: "r2", transactionId: "t2" } });
    expect(await keys()).toEqual(["openaide.agent.codex.env.OPENAI_API_KEY"]);
  });

  it("after an upgrade declares secrets saved before any index existed", async () => {
    const legacyKey = "openaide.agent.codex.env.OPENAI_API_KEY";
    const request = { type: "secret.keys.request", payload: { requestId: "keys" } } as const;

    // A store that can list its keys declares the old secret itself and holds nothing unlisted.
    const listing = memoryStore();
    listing.values.set(legacyKey, "saved-before-upgrade");
    const listed = await createWebSecretMessageHandler({
      ...listing,
      keys: async () => [...listing.values.keys()],
    })(request);
    expect(listed).toEqual({
      type: "secret.keys.result",
      payload: { requestId: "keys", keyIds: [legacyKey], unknownHolder: false },
    });

    // A store that cannot list them cannot declare the old secret, so it declares itself an
    // unknown holder and the App Server falls back to asking it.
    const opaque = memoryStore();
    opaque.values.set(legacyKey, "saved-before-upgrade");
    const unlisted = await createWebSecretMessageHandler(opaque)(request);
    expect(unlisted).toEqual({
      type: "secret.keys.result",
      payload: { requestId: "keys", keyIds: [], unknownHolder: true },
    });
    expect(JSON.stringify([listed, unlisted])).not.toContain("saved-before-upgrade");
  });
});
