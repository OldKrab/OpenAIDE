// @vitest-environment jsdom

import { useReducer } from "react";
import { afterEach, expect, it, vi } from "vitest";
import {
  AGENT_CREATE_CUSTOM,
  AGENT_DELETE_CUSTOM,
  AGENT_PROBE,
  SETTINGS_GET_AGENT_DETAILS,
  type AgentId,
  type BackendConnection,
} from "@openaide/app-server-client";
import { appReducer } from "../../state/appReducer";
import { createInitialState } from "../../state/store";
import { createSettingsCallbacks } from "../settingsCallbacks";
import { shouldConsumeAgentDeleteAck, shouldConsumeAgentSaveAck } from "./agentSettingsModel";
import { SettingsView } from "./SettingsView";
import {
  builtInAgent,
  button,
  buttons,
  click,
  customAgent,
  dialog,
  field,
  mount,
  mountSettings,
  pageText,
  settingsProps,
  settle,
  type,
  unmount,
} from "./settingsTestSupport";

afterEach(() => {
  unmount();
  vi.restoreAllMocks();
});

const agentsState = (agentDetails = [builtInAgent(), customAgent()]) => ({ activeTab: "agents" as const, agentDetails, loading: false });

it("groups Agents by origin and says how each one is doing", async () => {
  await mountSettings({
    state: agentsState([
      builtInAgent(),
      builtInAgent({ id: "claude", label: "Claude Code", icon: "claude", status: "failed", description: "Anthropic coding agent." }),
      customAgent({ enabled: false }),
    ]),
  });

  const groups = [...document.querySelectorAll('section[aria-label="Agents"] h2')].map((heading) => heading.textContent);
  expect(groups).toEqual(["Built-in", "Custom"]);
  const row = (name: string) => button(`Open ${name}`).closest("li")!.textContent;
  expect(row("Codex")).toContain("Connected");
  expect(row("Claude Code")).toContain("Needs attention");
  expect(row("Local Agent")).toContain("Off");
});

it("shows loading until the first Agent list arrives", async () => {
  await mountSettings({ state: { activeTab: "agents", loading: true } });

  expect(pageText()).toContain("Loading agents…");
  expect(buttons("Add agent")).toHaveLength(0);
});

it("turns an Agent on at once and asks before turning one off interrupts work", async () => {
  const onSetAgentEnabled = vi.fn()
    .mockResolvedValueOnce({ kind: "applied" })
    .mockResolvedValueOnce({ kind: "confirmation-required", runningTaskCount: 2 })
    .mockResolvedValueOnce({ kind: "applied" });
  await mountSettings({ onSetAgentEnabled, state: agentsState([builtInAgent(), customAgent({ enabled: false })]) });

  await click("Local Agent available");
  expect(onSetAgentEnabled).toHaveBeenLastCalledWith("custom-1", true);

  await click("Codex available");
  await settle();
  expect(onSetAgentEnabled).toHaveBeenLastCalledWith("codex", false, false);
  expect(dialog("Disable Codex")).not.toBeNull();
});

it("opens an Agent's page with its state and returns to the list", async () => {
  await mountSettings({
    state: agentsState([builtInAgent({ status: "failed", last_error_summary: "spawn codex ENOENT" })]),
  });

  await click("Open Codex");

  const page = document.querySelector('section[aria-label="Agent details"]')!;
  expect(page.querySelector("h1")?.textContent).toBe("Codex");
  expect(page.textContent).toContain("Built-in agent");
  expect(page.querySelector('[role="alert"]')?.textContent).toContain("Needs attention");
  expect(page.textContent).toContain("spawn codex ENOENT");
  // A built-in Agent's launch is fixed, so it is shown but cannot be edited.
  expect(page.querySelector('input[aria-label="Agent command"]')).toBeNull();

  await click("Back to Agents");

  expect(document.querySelector('section[aria-label="Agent details"]')).toBeNull();
  expect(button("Open Codex")).toBeTruthy();
});

it("adds a Custom Agent only once it has a name and a command", async () => {
  const onCreateCustomAgent = vi.fn();
  await mountSettings({ onCreateCustomAgent, state: agentsState([]) });

  await click("Add agent");
  expect(pageText()).toContain("Name and command are required.");
  await click("Add agent");
  expect(onCreateCustomAgent).not.toHaveBeenCalled();

  await type("Agent name", "Cursor");
  await type("Agent command", "agent acp");
  await click("Add variable");
  await type("Name", "API_BASE");
  await type("Value", "https://example.com");
  await click("Add agent");

  expect(onCreateCustomAgent).toHaveBeenCalledWith({
    label: "Cursor",
    icon: "bot",
    command_line: "agent acp",
    enabled: true,
    env: [{ name: "API_BASE", value: "https://example.com", secret: false }],
  });
});

it("offers Save only after an edit and saves a rename directly", async () => {
  const onUpdateCustomAgentMetadata = vi.fn(async () => ({ kind: "applied" as const }));
  await mountSettings({ onUpdateCustomAgentMetadata, state: agentsState() });

  await click("Open Local Agent");
  expect(buttons("Save")).toHaveLength(0);
  await type("Agent name", "Renamed");
  await click("Save");

  expect(onUpdateCustomAgentMetadata).toHaveBeenCalledWith({ agent_id: "custom-1", label: "Renamed", icon: "terminal", enabled: true });
});

it("asks before a launch change replaces the Agent", async () => {
  const onReplaceCustomAgent = vi.fn();
  await mountSettings({ onReplaceCustomAgent, state: agentsState() });

  await click("Open Local Agent");
  await type("Agent command", "local-agent --acp");
  await click("Save");

  expect(onReplaceCustomAgent).not.toHaveBeenCalled();
  expect(dialog("Replace Agent confirmation")?.textContent).toContain("create a new Agent identity");
  await click("Replace");
  await settle();

  expect(onReplaceCustomAgent).toHaveBeenCalledWith(expect.objectContaining({
    command_line: "local-agent --acp",
    confirmed: true,
    source_agent_id: "custom-1",
  }));
});

it("never drops unsaved edits by navigating away", async () => {
  await mountSettings({ state: agentsState() });

  await click("Open Local Agent");
  await type("Agent name", "Half typed");
  await click("Back to Agents");

  expect(dialog("Discard changes confirmation")).not.toBeNull();
  await click("Cancel", 1);
  expect((field("Agent name") as HTMLInputElement).value).toBe("Half typed");

  await click("Back to Agents");
  await click("Discard");
  await settle();

  expect(document.querySelector('section[aria-label="Agent details"]')).toBeNull();
});

it("deletes a Custom Agent only after confirmation", async () => {
  const onDeleteCustomAgent = vi.fn();
  await mountSettings({ onDeleteCustomAgent, state: agentsState() });

  await click("Open Local Agent");
  await click("Delete agent…");

  expect(onDeleteCustomAgent).not.toHaveBeenCalled();
  expect(dialog("Delete Agent confirmation")?.textContent).toContain("Delete Local Agent?");
  await click("Delete agent");
  await settle();

  expect(onDeleteCustomAgent).toHaveBeenCalledWith("custom-1");
});

it("consumes a save or delete acknowledgement only for the operation that is waiting on it", () => {
  expect(shouldConsumeAgentSaveAck({ savedAgentId: "agent-1", pendingSaveAgentId: "agent-1", hasDraft: true })).toBe(true);
  expect(shouldConsumeAgentSaveAck({ savedAgentId: "agent-1", pendingSaveAgentId: undefined, hasDraft: true })).toBe(false);
  expect(shouldConsumeAgentDeleteAck({ deletedAgentId: "agent-1", pendingDeleteAgentId: "agent-1" })).toBe(true);
  expect(shouldConsumeAgentDeleteAck({ deletedAgentId: "agent-1", pendingDeleteAgentId: "agent-2" })).toBe(false);
});

it("checks only the created Agent, lands on the list, and still allows deleting an Agent that failed", async () => {
  let createdAgentId: AgentId | undefined;
  let createdAgentProbed = false;
  let finishProbe!: () => void;
  const probeGate = new Promise<void>((resolve) => { finishProbe = resolve; });
  const request = vi.fn(async (method: string, params: Record<string, unknown>) => {
    if (method === AGENT_CREATE_CUSTOM) {
      createdAgentId = params.agentId as AgentId;
      return { agentId: createdAgentId, agents: protocolAgents([customProtocolAgent(createdAgentId, "disconnected")]) };
    }
    if (method === AGENT_PROBE) {
      expect(params).toEqual({ agentId: createdAgentId });
      await probeGate;
      createdAgentProbed = true;
      return { agents: protocolAgents([customProtocolAgent(createdAgentId!, "failed")]) };
    }
    if (method === SETTINGS_GET_AGENT_DETAILS) {
      // A catalog-wide read would hang on the bad process, so the targeted
      // check must happen before the cache read.
      if (!createdAgentProbed) return new Promise(() => undefined);
      return { generatedAt: "after-targeted-check", agents: [customSettingsAgent(createdAgentId!)] };
    }
    if (method === AGENT_DELETE_CUSTOM) return { agentId: createdAgentId, removedSecretEnv: [], agents: protocolAgents([]) };
    throw new Error(`Unexpected method: ${method}`);
  });
  await mount(<FlowHarness request={request as unknown as BackendConnection["request"]} />);

  await click("Add agent");
  await type("Agent name", "Cursor");
  await type("Agent command", "agent acp");
  await click("Add agent");
  await settle();

  expect(request.mock.calls.map(([method]) => method)).toEqual([AGENT_CREATE_CUSTOM, AGENT_PROBE]);
  // Saving a new Agent returns to the list so the new row is the visible result.
  expect(document.querySelector('section[aria-label="Agent details"]')).toBeNull();
  expect(button("Open Cursor")).toBeTruthy();

  finishProbe();
  await settle();

  expect(request.mock.calls.map(([method]) => method)).toEqual([AGENT_CREATE_CUSTOM, AGENT_PROBE, SETTINGS_GET_AGENT_DETAILS]);
  expect(button("Open Cursor").closest("li")!.textContent).toContain("Needs attention");

  await click("Open Cursor");
  await click("Delete agent…");
  await click("Delete agent");
  await settle();

  expect(request).toHaveBeenCalledWith(AGENT_DELETE_CUSTOM, { agentId: createdAgentId, expectedSecretEnv: [] });
});

function FlowHarness({ request }: { request: BackendConnection["request"] }) {
  const [state, dispatch] = useReducer(appReducer, undefined, () => {
    const initial = createInitialState();
    initial.settings.activeTab = "agents";
    initial.settings.agentDetails = [];
    return initial;
  });
  const callbacks = createSettingsCallbacks({
    backendConnection: { request },
    dispatch,
    setAgents: () => undefined,
    setPreferences: () => undefined,
    state,
  });
  return (
    <SettingsView
      {...settingsProps({
        onCreateCustomAgent: callbacks.createCustomAgent,
        onDeleteCustomAgent: callbacks.deleteCustomAgent,
        onReplaceCustomAgent: callbacks.replaceCustomAgent,
        onSetAgentEnabled: callbacks.setAgentEnabled,
        onUpdateCustomAgentMetadata: callbacks.updateCustomAgentMetadata,
        state: state.settings,
      })}
    />
  );
}

function protocolAgents(agents: ReturnType<typeof customProtocolAgent>[]) {
  return { generatedAt: "now", availability: "available" as const, agents };
}

function customProtocolAgent(agentId: AgentId, status: "disconnected" | "failed") {
  return {
    agentId,
    label: "Cursor",
    description: "Custom ACP stdio Agent",
    icon: "rocket",
    enabled: true,
    status,
    setupReason: null,
    capabilities: { resumeSessions: false, deleteSessions: false, forkSessions: false },
  };
}

function customSettingsAgent(agentId: AgentId) {
  return {
    agentId,
    label: "Cursor",
    enabled: true,
    sourceKind: "custom" as const,
    icon: "rocket",
    transport: "stdio" as const,
    status: "failed" as const,
    launchLabel: "agent",
    commandLine: "agent acp",
    env: [],
    description: "Custom ACP stdio Agent",
    capabilities: [],
    authMethods: [],
  };
}

it("shows the command a built-in Agent really runs", async () => {
  await mountSettings({
    state: agentsState([builtInAgent({ launch_label: "/usr/bin/npx -y @openaide/claude-agent-acp@1.2.3" })]),
  });

  await click("Open Codex");

  const page = document.querySelector('section[aria-label="Agent details"]')!;
  expect(page.querySelector("code")?.textContent).toBe("/usr/bin/npx -y @openaide/claude-agent-acp@1.2.3");
  expect(page.textContent).not.toContain("stdio");
});
