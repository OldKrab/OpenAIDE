import { describe, expect, it, vi } from "vitest";
import { createDiagnosticsLogger } from "./diagnostics";

describe("diagnostics logger", () => {
  it("keeps lifecycle metadata while redacting payload-shaped fields", () => {
    const sink = {
      info: vi.fn(),
      warn: vi.fn(),
      error: vi.fn(),
    };
    const logger = createDiagnosticsLogger("test", sink);

    logger.warn("operation_failed", {
      connection_id: "connection-1",
      duration_ms: 42,
      error_kind: "TypeError",
      prompt: "private prompt",
      token: "private token",
      error: "private error detail",
      url: "https://private.example.test",
    });

    const line = JSON.parse(sink.warn.mock.calls[0]?.[0] as string) as {
      fields: Record<string, unknown>;
    };
    expect(line.fields).toMatchObject({
      connection_id: "connection-1",
      duration_ms: 42,
      error_kind: "TypeError",
      prompt: "[redacted]",
      token: "[redacted]",
      error: "[redacted]",
      url: "[redacted]",
    });
  });
});
