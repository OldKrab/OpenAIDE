import { describe, expect, it } from "vitest";
import type { AgentCollectionSnapshot } from "@openaide/app-server-client";
import { agentOptionsFromProtocol, defaultAgentActionFromProtocol } from "./appServerAgents";

describe("App Server Agent state mapping", () => {
  it("maps backend Agent summaries to frontend presentation options", () => {
    expect(agentOptionsFromProtocol(agentCollection())).toEqual([
      expect.objectContaining({ id: "opencode", label: "OpenCode", icon: "opencode", enabled: true }),
      expect.objectContaining({ id: "custom.one", label: "Custom One", icon: "bot", enabled: true }),
    ]);
  });

  it("selects the backend default Agent for new task state", () => {
    expect(defaultAgentActionFromProtocol(agentCollection(), "codex")).toEqual({
      type: "newTask:agent",
      agentId: "custom.one",
      agentLabel: "Custom One",
    });
  });

  it("falls back to the first Agent when default is absent", () => {
    expect(defaultAgentActionFromProtocol(agentCollection({ defaultAgentId: null }))).toMatchObject({
      agentId: "opencode",
      agentLabel: "OpenCode",
    });
  });

  it("does not replace an already valid current Agent selection", () => {
    expect(defaultAgentActionFromProtocol(agentCollection(), "opencode")).toBeUndefined();
  });
});

function agentCollection(overrides: Partial<AgentCollectionSnapshot> = {}): AgentCollectionSnapshot {
  return {
    defaultAgentId: "custom.one" as never,
    agents: [
      { agentId: "opencode" as never, label: "OpenCode", status: "disconnected" },
      { agentId: "custom.one" as never, label: "Custom One", status: "disconnected" },
    ],
    ...overrides,
  };
}
