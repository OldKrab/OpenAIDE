import { describe, expect, it } from "vitest";
import type { AgentCollectionSnapshot } from "@openaide/app-server-client";
import { agentOptionsFromProtocol, fallbackAgentActionFromProtocol } from "./appServerAgents";

describe("App Server Agent state mapping", () => {
  it("maps backend Agent summaries to frontend presentation options", () => {
    expect(agentOptionsFromProtocol(agentCollection())).toEqual([
      expect.objectContaining({ id: "opencode", label: "OpenCode", icon: "opencode", enabled: true }),
      expect.objectContaining({ id: "custom.one", label: "Custom One", icon: "bot", enabled: true }),
    ]);
  });

  it("uses the first deterministic fallback when the selected Agent disappears", () => {
    expect(fallbackAgentActionFromProtocol(agentCollection(), "codex")).toEqual({
      type: "newTask:agent",
      agentId: "opencode",
      agentLabel: "OpenCode",
    });
  });

  it("does not replace an already valid current Agent selection", () => {
    expect(fallbackAgentActionFromProtocol(agentCollection(), "opencode")).toBeUndefined();
  });
});

function agentCollection(overrides: Partial<AgentCollectionSnapshot> = {}): AgentCollectionSnapshot {
  return {
    agents: [
      { agentId: "opencode" as never, label: "OpenCode", status: "disconnected" },
      { agentId: "custom.one" as never, label: "Custom One", status: "disconnected" },
    ],
    ...overrides,
  };
}
