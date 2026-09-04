import { useReducer } from "react";
import { act, create, type ReactTestInstance, type ReactTestRenderer } from "react-test-renderer";
import { beforeEach, describe, expect, it, vi } from "vitest";
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
import { AgentSettingsTab } from "./AgentSettingsTab";

describe("custom Agent Settings flow", () => {
  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  });

  it("checks only the created Agent, finishes saving a failed Agent, and still allows deletion", async () => {
    let createdAgentId: AgentId | undefined;
    let createdAgentProbed = false;
    let finishProbe!: () => void;
    const probeGate = new Promise<void>((resolve) => {
      finishProbe = resolve;
    });
    const request = vi.fn(async (method: string, params: Record<string, unknown>) => {
      if (method === AGENT_CREATE_CUSTOM) {
        createdAgentId = params.agentId as AgentId;
        return {
          agentId: createdAgentId,
          agents: protocolAgents([customProtocolAgent(createdAgentId, "disconnected")]),
        };
      }
      if (method === AGENT_PROBE) {
        expect(params).toEqual({ agentId: createdAgentId });
        await probeGate;
        createdAgentProbed = true;
        return { agents: protocolAgents([customProtocolAgent(createdAgentId!, "failed")]) };
      }
      if (method === SETTINGS_GET_AGENT_DETAILS) {
        // This models the production outage: a catalog-wide read would hang on
        // the bad process. The targeted check must happen before the cache read.
        if (!createdAgentProbed) return new Promise(() => undefined);
        return {
          generatedAt: "after-targeted-check",
          agents: [customSettingsAgent(createdAgentId!, "failed")],
        };
      }
      if (method === AGENT_DELETE_CUSTOM) {
        return {
          agentId: createdAgentId,
          removedSecretEnv: [],
          agents: protocolAgents([]),
        };
      }
      throw new Error(`Unexpected method: ${method}`);
    });
    const view = renderFlow(request as unknown as BackendConnection["request"]);

    act(() => buttonByText(view.root, "Add agent").props.onClick());
    const [nameInput, commandInput] = view.root.findAllByType("input").filter((input) => input.props.value === "");
    act(() => nameInput.props.onChange({ currentTarget: { value: "Cursor" } }));
    act(() => commandInput.props.onChange({ currentTarget: { value: "agent acp" } }));
    act(() => buttonByText(view.root, "Save").props.onClick());
    await settle();

    expect(request.mock.calls.map(([method]) => method)).toEqual([
      AGENT_CREATE_CUSTOM,
      AGENT_PROBE,
    ]);
    expect(buttonsByText(view.root, "Saving and checking…")).toHaveLength(0);
    expect(buttonByText(view.root, "Delete")).toBeTruthy();

    finishProbe();
    await settle();
    expect(request.mock.calls.map(([method]) => method)).toEqual([
      AGENT_CREATE_CUSTOM,
      AGENT_PROBE,
      SETTINGS_GET_AGENT_DETAILS,
    ]);
    expect(textContent(view.root)).toContain("Connection check failed.");

    act(() => buttonByText(view.root, "Delete").props.onClick());
    act(() => buttonByText(view.root, "Confirm delete").props.onClick());
    await settle();

    expect(request).toHaveBeenCalledWith(AGENT_DELETE_CUSTOM, {
      agentId: createdAgentId,
      expectedSecretEnv: [],
    });
  });
});

function renderFlow(request: BackendConnection["request"]) {
  let view: ReactTestRenderer | undefined;
  act(() => {
    view = create(<AgentSettingsFlowHarness request={request} />);
  });
  return view!;
}

function AgentSettingsFlowHarness({ request }: { request: BackendConnection["request"] }) {
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
    <AgentSettingsTab
      agents={state.settings.agentDetails ?? []}
      deletedAgentId={state.settings.deletedAgentId}
      onAuthenticate={callbacks.authenticateAgent}
      onCreateCustomAgent={callbacks.createCustomAgent}
      onDeleteCustomAgent={callbacks.deleteCustomAgent}
      onReplaceCustomAgent={callbacks.replaceCustomAgent}
      onSetAgentEnabled={callbacks.setAgentEnabled}
      onUpdateCustomAgentMetadata={callbacks.updateCustomAgentMetadata}
      saveError={state.settings.error}
      savedAgentId={state.settings.savedAgentId}
    />
  );
}

function protocolAgents(customAgents: ReturnType<typeof customProtocolAgent>[]) {
  return {
    generatedAt: "now",
    availability: "available" as const,
    agents: customAgents,
  };
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
    capabilities: {
      resumeSessions: false,
      deleteSessions: false,
      forkSessions: false,
    },
  };
}

function customSettingsAgent(agentId: AgentId, status: "failed") {
  return {
    agentId,
    label: "Cursor",
    enabled: true,
    sourceKind: "custom" as const,
    icon: "rocket",
    transport: "stdio" as const,
    status,
    launchLabel: "agent",
    commandLine: "agent acp",
    env: [],
    description: "Custom ACP stdio Agent",
    capabilities: [],
    authMethods: [],
  };
}

function buttonByText(root: ReactTestInstance, text: string) {
  return root.findAllByType("button").find((button) => textContent(button) === text)!;
}

function buttonsByText(root: ReactTestInstance, text: string) {
  return root.findAllByType("button").filter((button) => textContent(button) === text);
}

function textContent(node: ReactTestInstance): string {
  return node.children.map((child) => (typeof child === "string" ? child : textContent(child))).join("");
}

async function settle() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
  });
}
