import { afterEach, describe, expect, it, vi } from "vitest";

const originalCrypto = globalThis.crypto;

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("task mutation intents", () => {
  it("creates send idempotency keys that do not reset to durable receipt keys after reload", async () => {
    vi.stubGlobal("crypto", { ...originalCrypto, randomUUID: vi.fn(() => "uuid-1") });
    vi.resetModules();
    const firstModule = await import("./taskMutationIntents");

    expect(firstModule.createTaskSendIdempotencyKey()).toBe("frontend-send-uuid-1");

    vi.stubGlobal("crypto", { ...originalCrypto, randomUUID: vi.fn(() => "uuid-2") });
    vi.resetModules();
    const reloadedModule = await import("./taskMutationIntents");

    expect(reloadedModule.createTaskSendIdempotencyKey()).toBe("frontend-send-uuid-2");
  });

  it("keeps generating usable keys when crypto.randomUUID is unavailable", async () => {
    vi.stubGlobal("crypto", {});
    vi.resetModules();
    const module = await import("./taskMutationIntents");

    expect(module.createTaskSendIdempotencyKey()).toMatch(/^frontend-send-/);
  });
});
