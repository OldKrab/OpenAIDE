import { describe, expect, it, vi } from "vitest";
import { customAgentSecretKey } from "../settings/agents";
import { secretEnv } from "./hostAgentSecrets";

describe("Agent secret host bridge", () => {
  it("returns only requested secret environment values", async () => {
    const get = vi.fn(async (key: string) =>
      key === customAgentSecretKey("custom.one", "TOKEN") ? "secret-token" : undefined,
    );

    await expect(secretEnv({ agent_id: "custom.one", names: ["TOKEN"] }, { get } as never)).resolves.toEqual({
      env: { TOKEN: "secret-token" },
    });
  });

  it("rejects missing secret values", async () => {
    await expect(secretEnv({ agent_id: "custom.one", names: ["TOKEN"] }, { get: vi.fn(async () => undefined) } as never)).rejects.toThrow(
      "Missing secret",
    );
  });
});
