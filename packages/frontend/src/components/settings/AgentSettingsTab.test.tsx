import { act, create, type ReactTestInstance } from "react-test-renderer";
import type { ReactTestRenderer } from "react-test-renderer";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentSettingsRecord } from "@openaide/app-shell-contracts";
import { AgentSettingsTab } from "./AgentSettingsTab";

describe("AgentSettingsTab interactions", () => {
  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  });

  it("updates edited custom Agent metadata without launch fields", () => {
    const onUpdateCustomAgentMetadata = vi.fn();
    const view = renderAgentSettings({ agents: [customAgent("custom.local")], onUpdateCustomAgentMetadata });
    const nameInput = inputByProps(view.root, { value: "Custom Agent" });

    act(() => {
      nameInput.props.onChange({ currentTarget: { value: "Updated Agent" } });
    });
    act(() => {
      buttonByText(view.root, "Save").props.onClick();
    });

    expect(onUpdateCustomAgentMetadata).toHaveBeenCalledWith({
      agent_id: "custom.local",
      label: "Updated Agent",
      icon: "bot",
      enabled: true,
    });
  });

  it("requires confirmation before saving launch-changing custom Agent edits", () => {
    const onReplaceCustomAgent = vi.fn();
    const view = renderAgentSettings({ agents: [customAgent("custom.local")], onReplaceCustomAgent });
    const commandInput = inputByProps(view.root, { value: "agent run" });

    act(() => {
      commandInput.props.onChange({ currentTarget: { value: "agent run --new" } });
    });
    act(() => {
      buttonByText(view.root, "Save").props.onClick();
    });

    expect(onReplaceCustomAgent).not.toHaveBeenCalled();
    expect(buttonByText(view.root, "Confirm replace")).toBeTruthy();
    expect(textContent(view.root)).toContain("Launch changes create a new Agent identity");

    act(() => {
      buttonByText(view.root, "Confirm replace").props.onClick();
    });

    expect(onReplaceCustomAgent).toHaveBeenCalledWith(expect.objectContaining({
      source_agent_id: "custom.local",
      command_line: "agent run --new",
      confirmed: true,
    }));
  });

  it("starts Add Custom Agent as a normal new draft", () => {
    const view = renderAgentSettings({ agents: [builtInAgent("codex")] });

    act(() => {
      buttonByText(view.root, "Add agent").props.onClick();
    });

    expect(textContent(view.root)).toContain("Add Custom Agent");
    expect(buttonByText(view.root, "Save")).toBeTruthy();
    expect(buttonsByText(view.root, "Confirm replace")).toHaveLength(0);
    expect(textContent(view.root)).not.toContain("Launch changes create a new Agent identity");
  });

  it("puts required new Agent launch fields before icon customization", () => {
    const view = renderAgentSettings({ agents: [builtInAgent("codex")] });

    act(() => {
      buttonByText(view.root, "Add agent").props.onClick();
    });

    const nameInput = view.root.findByProps({ "aria-label": "Agent name" });
    const commandInput = view.root.findByProps({ "aria-label": "Agent command" });
    const iconPicker = view.root.findByProps({ "aria-label": "Agent icon", role: "radiogroup" });

    expect(nodeOrder(view.root, nameInput)).toBeLessThan(nodeOrder(view.root, iconPicker));
    expect(nodeOrder(view.root, commandInput)).toBeLessThan(nodeOrder(view.root, iconPicker));
  });

  it("keeps blank new custom Agent drafts local until required launch fields are filled", () => {
    const onCreateCustomAgent = vi.fn();
    const view = renderAgentSettings({ agents: [builtInAgent("codex")], onCreateCustomAgent });

    act(() => {
      buttonByText(view.root, "Add agent").props.onClick();
    });

    expect(buttonByText(view.root, "Save").props.disabled).toBe(true);
    expect(textContent(view.root)).toContain("Name and command are required.");

    const [nameInput, commandInput] = view.root.findAllByType("input").filter((input) => input.props.value === "");
    act(() => {
      nameInput.props.onChange({ currentTarget: { value: "Local Agent" } });
    });
    act(() => {
      commandInput.props.onChange({ currentTarget: { value: "local-agent --stdio" } });
    });
    act(() => {
      buttonByText(view.root, "Save").props.onClick();
    });

    expect(onCreateCustomAgent).toHaveBeenCalledWith(expect.objectContaining({
      label: "Local Agent",
      command_line: "local-agent --stdio",
    }));
  });

  it("labels Agent and environment add actions distinctly", () => {
    const view = renderAgentSettings({ agents: [customAgent("custom.local")] });

    expect(buttonByText(view.root, "Add agent")).toBeTruthy();
    expect(buttonByText(view.root, "Add variable")).toBeTruthy();
    expect(buttonsByText(view.root, "Add")).toHaveLength(0);
  });

  it("cancels a new custom Agent draft back to the selected Agent", () => {
    const view = renderAgentSettings({ agents: [builtInAgent("codex")] });

    act(() => {
      buttonByText(view.root, "Add agent").props.onClick();
    });
    expect(textContent(view.root)).toContain("Add Custom Agent");
    expect(textContent(view.root)).toContain("Name and command are required.");

    act(() => {
      buttonByText(view.root, "Cancel").props.onClick();
    });

    expect(textContent(view.root)).toContain("Codex");
    expect(textContent(view.root)).not.toContain("Add Custom Agent");
    expect(textContent(view.root)).not.toContain("Name and command are required.");
  });

  it("keeps an existing custom Agent draft open when another Agent save is acknowledged", () => {
    const onUpdateCustomAgentMetadata = vi.fn();
    const view = renderAgentSettings({ agents: [customAgent("custom.local")], onUpdateCustomAgentMetadata });
    const nameInput = inputByProps(view.root, { value: "Custom Agent" });

    act(() => {
      nameInput.props.onChange({ currentTarget: { value: "Edited Agent" } });
    });
    act(() => {
      buttonByText(view.root, "Save").props.onClick();
    });
    act(() => {
      view.update(
        <AgentSettingsTab
          agents={[customAgent("custom.local")]}
          authPending={false}
          onAuthenticate={vi.fn()}
          onCreateCustomAgent={vi.fn()}
          onDeleteCustomAgent={vi.fn()}
          onReplaceCustomAgent={vi.fn()}
          onSetAgentEnabled={vi.fn()}
          onUpdateCustomAgentMetadata={onUpdateCustomAgentMetadata}
          savedAgentId="other.agent"
        />,
      );
    });

    expect(inputByProps(view.root, { value: "Edited Agent" })).toBeTruthy();
  });

  it("prevents Agent selection from silently discarding an unsaved draft", () => {
    const view = renderAgentSettings({
      agents: [customAgent("custom.local"), builtInAgent("codex")],
    });

    act(() => {
      inputByProps(view.root, { value: "Custom Agent" }).props.onChange({ currentTarget: { value: "Unsaved name" } });
    });

    const agentRows = view.root.findByProps({ "aria-label": "Agents", role: "list" }).findAllByType("button");
    expect(agentRows.every((button) => button.props.disabled === true)).toBe(true);
    expect(buttonByText(view.root, "Add agent").props.disabled).toBe(true);
    expect(textContent(view.root)).toContain("Save or cancel changes before selecting another agent.");
    expect(inputByProps(view.root, { value: "Unsaved name" })).toBeTruthy();
  });

  it("requires a second click before deleting a custom Agent", () => {
    const onDeleteCustomAgent = vi.fn();
    const view = renderAgentSettings({ agents: [customAgent("custom.local")], onDeleteCustomAgent });

    act(() => {
      buttonByText(view.root, "Delete").props.onClick();
    });
    expect(onDeleteCustomAgent).not.toHaveBeenCalled();
    expect(buttonByText(view.root, "Confirm delete")).toBeTruthy();

    act(() => {
      buttonByText(view.root, "Confirm delete").props.onClick();
    });
    expect(onDeleteCustomAgent).toHaveBeenCalledWith("custom.local");
  });

  it("toggles built-in Agent availability through the parent callback", () => {
    const onSetAgentEnabled = vi.fn();
    const view = renderAgentSettings({ agents: [builtInAgent("codex")], onSetAgentEnabled });
    const enabledToggle = inputByProps(view.root, { checked: true, type: "checkbox" });

    act(() => {
      enabledToggle.props.onChange({ currentTarget: { checked: false } });
    });

    expect(onSetAgentEnabled).toHaveBeenCalledWith("codex", false);
  });

  it("describes disabled built-in Agent availability as disabled", () => {
    const view = renderAgentSettings({
      agents: [builtInAgent("codex", { enabled: false, status: "disabled" })],
    });

    expect(textContent(view.root)).toContain("Disabled");
    expect(textContent(view.root)).toContain("Agent is hidden from new task selection.");
    expect(textContent(view.root)).not.toContain("Agent is available to be selected and used.");
  });

  it("explains enabled Agents whose status has not been checked yet", () => {
    const view = renderAgentSettings({
      agents: [builtInAgent("codex", { enabled: true, status: "disconnected" })],
    });

    expect(textContent(view.root)).toContain("Status check needed. Refresh to verify this agent.");
    expect(textContent(view.root)).not.toContain("Status has not been checked.");
  });

  it("preserves the empty Agent list header text from the original tab", () => {
    const view = renderAgentSettings({ agents: [] });
    const details = view.root.findByProps({ className: "agent-detail-identity" });

    expect(textContent(details)).toBe("Built-in");
  });
});

function renderAgentSettings({
  agents,
  onCreateCustomAgent = vi.fn(),
  onDeleteCustomAgent = vi.fn(),
  onReplaceCustomAgent = vi.fn(),
  onSetAgentEnabled = vi.fn(),
  onUpdateCustomAgentMetadata = vi.fn(),
}: {
  agents: AgentSettingsRecord[];
  onCreateCustomAgent?: Parameters<typeof AgentSettingsTab>[0]["onCreateCustomAgent"];
  onDeleteCustomAgent?: (agentId: string) => void;
  onReplaceCustomAgent?: Parameters<typeof AgentSettingsTab>[0]["onReplaceCustomAgent"];
  onSetAgentEnabled?: (agentId: string, enabled: boolean) => void;
  onUpdateCustomAgentMetadata?: Parameters<typeof AgentSettingsTab>[0]["onUpdateCustomAgentMetadata"];
}) {
  let view: ReactTestRenderer | undefined;
  act(() => {
    view = create(
      <AgentSettingsTab
        agents={agents}
        authPending={false}
        onAuthenticate={vi.fn()}
        onCreateCustomAgent={onCreateCustomAgent}
        onDeleteCustomAgent={onDeleteCustomAgent}
        onReplaceCustomAgent={onReplaceCustomAgent}
        onSetAgentEnabled={onSetAgentEnabled}
        onUpdateCustomAgentMetadata={onUpdateCustomAgentMetadata}
      />,
    );
  });
  return view!;
}

function buttonByText(root: ReactTestInstance, text: string) {
  return root.findAllByType("button").find((button) => textContent(button) === text)!;
}

function buttonsByText(root: ReactTestInstance, text: string) {
  return root.findAllByType("button").filter((button) => textContent(button) === text);
}

function inputByProps(root: ReactTestInstance, props: Record<string, unknown>) {
  return root.findAllByType("input").find((input) =>
    Object.entries(props).every(([key, value]) => input.props[key] === value),
  )!;
}

function nodeOrder(root: ReactTestInstance, target: ReactTestInstance): number {
  let order = 0;
  let found = -1;
  const visit = (node: ReactTestInstance) => {
    if (found >= 0) {
      return;
    }
    if (node === target) {
      found = order;
      return;
    }
    order += 1;
    node.children.forEach((child) => {
      if (typeof child !== "string") {
        visit(child);
      }
    });
  };
  visit(root);
  return found;
}

function textContent(node: ReactTestInstance): string {
  return node.children.map((child) => (typeof child === "string" ? child : textContent(child))).join("");
}

function customAgent(id: string): AgentSettingsRecord {
  return {
    id,
    label: "Custom Agent",
    enabled: true,
    scope: "global",
    source_kind: "custom",
    icon: "bot",
    transport: "stdio",
    status: "ready",
    launch_label: "agent run",
    command_line: "agent run",
    description: "Custom Agent",
    capabilities: [],
    auth_methods: [],
    env: [{ name: "TOKEN", value: "abc", secret: false }],
  };
}

function builtInAgent(id: string, overrides: Partial<AgentSettingsRecord> = {}): AgentSettingsRecord {
  return {
    id,
    label: "Codex",
    enabled: true,
    scope: "global",
    source_kind: "built_in",
    icon: "openai",
    transport: "stdio",
    status: "ready",
    launch_label: "codex",
    command_line: undefined,
    description: "Codex Agent",
    capabilities: [],
    auth_methods: [],
    ...overrides,
  };
}
