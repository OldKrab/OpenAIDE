import { describe, expect, it } from "vitest";
import {
  clearPendingTaskSendRecovery,
  readPendingTaskSendRecovery,
  savePendingTaskSendRecovery,
  type PendingTaskSendRecovery,
} from "./pendingTaskSendRecovery";

describe("pending Task send recovery", () => {
  it("does not expose an attempt from another state root when Task ids collide", () => {
    const storage = memoryStorage();
    const first = recovery("client-a", "task-a", "send-root-1", "root-1");
    const second = recovery("client-a", "task-a", "send-root-2", "root-2");

    savePendingTaskSendRecovery(first, storage);
    savePendingTaskSendRecovery(second, storage);

    expect(readPendingTaskSendRecovery("root-1", "client-a", "task-a", storage)).toEqual(first);
    expect(readPendingTaskSendRecovery("root-2", "client-a", "task-a", storage)).toEqual(second);
  });

  it("keeps independent attempts bound to their client instance and Task", () => {
    const storage = memoryStorage();
    const first = recovery("client-a", "task-a", "send-a");
    const second = recovery("client-b", "task-b", "send-b");

    savePendingTaskSendRecovery(first, storage);
    savePendingTaskSendRecovery(second, storage);

    expect(readPendingTaskSendRecovery("root-a", "client-a", "task-a", storage)).toEqual(first);
    expect(readPendingTaskSendRecovery("root-a", "client-b", "task-b", storage)).toEqual(second);
    expect(readPendingTaskSendRecovery("root-a", "client-a", "task-b", storage)).toBeUndefined();
    expect(storage.length).toBe(2);
  });

  it("clears only the matching stable attempt", () => {
    const storage = memoryStorage();
    savePendingTaskSendRecovery(recovery("client-a", "task-a", "send-new"), storage);

    clearPendingTaskSendRecovery("root-a", "client-a", "task-a", "send-old" as never, storage);
    expect(readPendingTaskSendRecovery("root-a", "client-a", "task-a", storage)?.idempotencyKey).toBe("send-new");

    clearPendingTaskSendRecovery("root-a", "client-a", "task-a", "send-new" as never, storage);
    expect(readPendingTaskSendRecovery("root-a", "client-a", "task-a", storage)).toBeUndefined();
  });

  it("rejects malformed or internally inconsistent recovery payloads", () => {
    const storage = memoryStorage();
    const clientInstanceId = "client-malformed";
    const taskId = "task-malformed";
    const stateRootId = "root-a";
    const key = `openaide:pending-task-send:v3:${encodeURIComponent(stateRootId)}:${encodeURIComponent(clientInstanceId)}:${encodeURIComponent(taskId)}`;
    storage.setItem(key, JSON.stringify({
      clientInstanceId,
      idempotencyKey: "send-a",
      message: { text: "different text", attachments: ["handle-a"] },
      renderState: {
        prompt: "hello",
        context: [{
          app_server_handle_id: "handle-b",
          kind: "file",
          label: "notes.md",
          local_id: "attachment-a",
        }],
      },
      schemaVersion: 3,
      stateRootId,
      taskId,
      taskRevision: 1,
    }));

    expect(readPendingTaskSendRecovery(stateRootId, clientInstanceId, taskId, storage)).toBeUndefined();
  });

  it("quarantines legacy recovery records that were not scoped to a state root", () => {
    const storage = memoryStorage();
    const legacyKey = "openaide:pending-task-send:v2:client-legacy:task-legacy";
    storage.setItem(legacyKey, JSON.stringify({
      ...recovery("client-legacy", "task-legacy", "send-legacy"),
      schemaVersion: 2,
      stateRootId: undefined,
    }));

    expect(readPendingTaskSendRecovery("root-a", "client-legacy", "task-legacy", storage)).toBeUndefined();
    expect(storage.getItem(legacyKey)).toBeNull();
  });

  it("keeps the current-process attempt locked when session storage is unavailable", () => {
    const storage = throwingStorage();
    const pending = recovery("client-blocked", "task-blocked", "send-blocked");

    savePendingTaskSendRecovery(pending, storage);
    expect(readPendingTaskSendRecovery("root-a", "client-blocked", "task-blocked", storage)).toEqual(pending);

    clearPendingTaskSendRecovery("root-a", "client-blocked", "task-blocked", "send-blocked" as never, storage);
    expect(readPendingTaskSendRecovery("root-a", "client-blocked", "task-blocked", storage)).toBeUndefined();
  });

  it("keeps the current-process attempt when storage accepts reads but rejects a full write", () => {
    const storage = quotaExceededStorage();
    const pending = recovery("client-full", "task-full", "send-full");

    savePendingTaskSendRecovery(pending, storage);

    expect(readPendingTaskSendRecovery("root-a", "client-full", "task-full", storage)).toEqual(pending);
    clearPendingTaskSendRecovery("root-a", "client-full", "task-full", "send-full" as never, storage);
    expect(readPendingTaskSendRecovery("root-a", "client-full", "task-full", storage)).toBeUndefined();
  });

  it("does not resurrect a stale persisted attempt after process-local clear and replacement", () => {
    const storage = mutableFailureStorage();
    const first = recovery("client-stale", "task-stale", "send-a");
    const replacement = recovery("client-stale", "task-stale", "send-b");
    savePendingTaskSendRecovery(first, storage);
    storage.blockMutations();

    clearPendingTaskSendRecovery("root-a", "client-stale", "task-stale", "send-a" as never, storage);
    savePendingTaskSendRecovery(replacement, storage);

    expect(storage.persistedRecord()?.idempotencyKey).toBe("send-a");
    expect(readPendingTaskSendRecovery("root-a", "client-stale", "task-stale", storage)).toEqual(replacement);
  });

  it("does not persist pasted-image preview bytes with the send recovery receipt", () => {
    const storage = memoryStorage();
    const pending: PendingTaskSendRecovery = {
      ...recovery("client-image", "task-image", "send-image"),
      message: { text: "inspect", attachments: ["handle-image" as never] },
      renderState: {
        prompt: "inspect",
        context: [{
          app_server_handle_id: "handle-image" as never,
          kind: "file",
          label: "pasted.png",
          local_id: "attachment-image",
          preview_url: `data:image/png;base64,${"a".repeat(100_000)}`,
        }],
      },
    };

    savePendingTaskSendRecovery(pending, storage);

    const raw = storage.getItem(storage.key(0) ?? "");
    expect(raw).not.toContain("data:image/png");
    expect(readPendingTaskSendRecovery("root-a", "client-image", "task-image", storage))
      .toEqual({
        ...pending,
        renderState: {
          ...pending.renderState,
          context: [expect.not.objectContaining({ preview_url: expect.anything() })],
        },
      });
  });
});

function recovery(
  clientInstanceId: string,
  taskId: string,
  idempotencyKey: string,
  stateRootId = "root-a",
): PendingTaskSendRecovery {
  return {
    clientInstanceId: clientInstanceId as never,
    idempotencyKey: idempotencyKey as never,
    message: { text: "hello" },
    renderState: { prompt: "hello", context: [] },
    stateRootId: stateRootId as never,
    taskId,
    taskRevision: 1,
  };
}

function memoryStorage(): Storage {
  const values = new Map<string, string>();
  return {
    get length(): number {
      return values.size;
    },
    clear: () => values.clear(),
    getItem: (key) => values.get(key) ?? null,
    key: (index) => [...values.keys()][index] ?? null,
    removeItem: (key) => {
      values.delete(key);
    },
    setItem: (key, value) => {
      values.set(key, value);
    },
  };
}

function throwingStorage(): Storage {
  return {
    get length(): number {
      throw new Error("storage blocked");
    },
    clear: () => { throw new Error("storage blocked"); },
    getItem: () => { throw new Error("storage blocked"); },
    key: () => { throw new Error("storage blocked"); },
    removeItem: () => { throw new Error("storage blocked"); },
    setItem: () => { throw new Error("storage blocked"); },
  };
}

function quotaExceededStorage(): Storage {
  const values = new Map<string, string>();
  return {
    get length() {
      return values.size;
    },
    clear: () => values.clear(),
    getItem: (key) => values.get(key) ?? null,
    key: (index) => [...values.keys()][index] ?? null,
    removeItem: (key) => {
      values.delete(key);
    },
    setItem: () => {
      throw new DOMException("Storage quota exceeded", "QuotaExceededError");
    },
  };
}

function mutableFailureStorage(): Storage & {
  blockMutations: () => void;
  persistedRecord: () => { idempotencyKey?: string } | undefined;
} {
  const values = new Map<string, string>();
  let blocked = false;
  return {
    get length() {
      return values.size;
    },
    blockMutations: () => {
      blocked = true;
    },
    clear: () => {
      if (blocked) throw new Error("storage mutation blocked");
      values.clear();
    },
    getItem: (key) => values.get(key) ?? null,
    key: (index) => [...values.keys()][index] ?? null,
    persistedRecord: () => {
      const raw = [...values.values()][0];
      return raw ? JSON.parse(raw) as { idempotencyKey?: string } : undefined;
    },
    removeItem: (key) => {
      if (blocked) throw new Error("storage mutation blocked");
      values.delete(key);
    },
    setItem: (key, value) => {
      if (blocked) throw new Error("storage mutation blocked");
      values.set(key, value);
    },
  };
}
