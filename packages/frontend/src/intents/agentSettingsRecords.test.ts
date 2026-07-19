import { describe, expect, it } from "vitest";
import type { AgentId } from "@openaide/app-server-client";
import { agentSettingsRecordFromProtocol } from "./agentSettingsRecords";

describe("agentSettingsRecordFromProtocol", () => {
  it("preserves the setup reason needed for contextual recovery", () => {
    const record = agentSettingsRecordFromProtocol({
      agentId: "codex" as AgentId,
      label: "Codex",
      enabled: true,
      sourceKind: "builtIn",
      icon: "openai",
      transport: "stdio",
      status: "setupRequired",
      setupReason: "nodeJsRequired",
      launchLabel: "Built-in stdio launch policy",
      description: "Built-in ACP Agent.",
    });

    expect(record.setup_reason).toBe("nodeJsRequired");
  });
});
