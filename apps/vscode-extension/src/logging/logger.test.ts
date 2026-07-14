import { afterEach, describe, expect, it, vi } from "vitest";
import { ExtensionLogger } from "./logger";

describe("ExtensionLogger", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("fully redacts arbitrary values stored under sensitive field names", () => {
    const output: string[] = [];
    vi.spyOn(console, "warn").mockImplementation((line) => output.push(String(line)));

    new ExtensionLogger("test").warn("operation failed", {
      error: "arbitrary-private-detail token-value",
      error_kind: "transport_closed",
      error_name: "TypeError",
      task_id: "task_1",
    });

    expect(output).toHaveLength(1);
    expect(output[0]).not.toContain("arbitrary-private-detail");
    expect(output[0]).not.toContain("token-value");
    expect(output[0]).toContain("transport_closed");
    expect(output[0]).toContain("TypeError");
    expect(output[0]).toContain("task_1");
    expect(JSON.parse(output[0])).toMatchObject({
      event: "operation_failed",
      level: "warn",
      scope: "test",
    });
    expect(JSON.parse(output[0])).not.toHaveProperty("message");
  });
});
