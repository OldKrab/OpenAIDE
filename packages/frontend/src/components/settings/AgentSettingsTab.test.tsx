import { act, create, type ReactTestInstance } from "react-test-renderer";
import type { ReactTestRenderer } from "react-test-renderer";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentSettingsRecord } from "@openaide/app-shell-contracts";
import type { AgentRecoveryActions } from "../AgentRecovery";
import { agentLeftLaunching } from "./agentSettingsModel";
import { AgentSettingsTab } from "./AgentSettingsTab";
import { installFrontendShell } from "../../services/frontendShell";

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
    const view = renderAgentSettings({ agents: [builtInAgent("codex")], openFirst: false });

    act(() => {
      buttonByText(view.root, "Add agent").props.onClick();
    });

    expect(textContent(view.root)).toContain("Add Custom Agent");
    expect(buttonByText(view.root, "Save")).toBeTruthy();
    expect(buttonsByText(view.root, "Confirm replace")).toHaveLength(0);
    expect(textContent(view.root)).not.toContain("Launch changes create a new Agent identity");
  });

  it("puts required new Agent launch fields before icon customization", () => {
    const view = renderAgentSettings({ agents: [builtInAgent("codex")], openFirst: false });

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
    const view = renderAgentSettings({ agents: [builtInAgent("codex")], onCreateCustomAgent, openFirst: false });

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

  it("shows that a new custom Agent is being saved and checked", () => {
    const view = renderAgentSettings({ agents: [builtInAgent("codex")], openFirst: false });

    act(() => {
      buttonByText(view.root, "Add agent").props.onClick();
    });
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

    expect(buttonByText(view.root, "Saving and checking…").props.disabled).toBe(true);
  });

  it("labels Agent and environment add actions distinctly", () => {
    const view = renderAgentSettings({ agents: [customAgent("custom.local")], openFirst: false });

    expect(buttonByText(view.root, "Add agent")).toBeTruthy();
    act(() => view.root.findByProps({ className: "agent-catalog-row" }).props.onClick());
    expect(buttonByText(view.root, "Add variable")).toBeTruthy();
    expect(buttonsByText(view.root, "Add")).toHaveLength(0);
  });

  it("cancels a new custom Agent draft back to the selected Agent", () => {
    const view = renderAgentSettings({ agents: [builtInAgent("codex")], openFirst: false });

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

  it("prevents leaving an Agent while it has an unsaved draft", () => {
    const view = renderAgentSettings({
      agents: [customAgent("custom.local"), builtInAgent("codex")],
    });

    act(() => {
      inputByProps(view.root, { value: "Custom Agent" }).props.onChange({ currentTarget: { value: "Unsaved name" } });
    });

    expect(buttonByText(view.root, "Back to Agents").props.disabled).toBe(true);
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

  it("toggles Agent availability directly from the catalog", () => {
    const onSetAgentEnabled = vi.fn();
    const view = renderAgentSettings({ agents: [builtInAgent("codex")], onSetAgentEnabled, openFirst: false });
    const availabilityToggle = view.root.findByProps({ "aria-label": "Codex available", type: "checkbox" });

    act(() => {
      availabilityToggle.props.onChange({ currentTarget: { checked: false } });
    });

    expect(onSetAgentEnabled).toHaveBeenCalledWith("codex", false);
  });

  it("describes disabled built-in Agent availability as disabled", () => {
    const view = renderAgentSettings({
      agents: [builtInAgent("codex", { enabled: false, status: "disabled" })],
    });

    expect(textContent(view.root)).toContain("Disabled");
    expect(textContent(view.root)).not.toContain("Agent is hidden from new task selection.");
    expect(textContent(view.root)).not.toContain("Agent is available to be selected and used.");
  });

  it("offers a concise connection action for an idle disconnected Agent", () => {
    const onRetry = vi.fn(async () => true);
    const view = renderAgentSettings({
      agents: [builtInAgent("codex", { enabled: true, status: "disconnected" })],
      recoveryActions: recoveryActions({ onRetry }),
    });

    expect(textContent(view.root)).toContain("ConnectionNot connectedConnect");
    expect(textContent(view.root)).not.toContain("Action required");
    expect(textContent(view.root)).not.toContain("Status check needed");
    expect(textContent(view.root)).not.toContain("disconnected");
    act(() => buttonByText(view.root, "Connect").props.onClick());
    expect(onRetry).toHaveBeenCalledWith("codex");
  });

  it("does not present idle disconnected as a broken Agent in the catalog", () => {
    const view = renderAgentSettings({
      agents: [
        builtInAgent("codex", { enabled: true, status: "disconnected" }),
        builtInAgent("opencode", { enabled: false, status: "disabled" }),
      ],
      openFirst: false,
    });

    expect(textContent(view.root)).not.toContain("disconnected");
    expect(textContent(view.root)).toContain("Off");
  });

  it("shows the shared managed integration activity in the catalog and Agent details", () => {
    const view = renderAgentSettings({
      agents: [builtInAgent("codex", { status: "installing" })],
      openFirst: false,
    });

    const catalogActivity = view.root.findByProps({ className: "agent-library-state installing" });
    expect(catalogActivity.props["aria-busy"]).toBe(true);
    expect(textContent(catalogActivity)).toBe("Installing the Codex integration…");

    act(() => view.root.findByProps({ className: "agent-catalog-row" }).props.onClick());

    const detailActivity = view.root.findByProps({ className: "agent-page-status installing" });
    expect(detailActivity.props["aria-busy"]).toBe(true);
    expect(textContent(detailActivity)).toBe("Installing the Codex integration…");
  });

  it("offers recovery when the selected Agent requires setup", () => {
    const onRetry = vi.fn(async () => true);
    const view = renderAgentSettings({
      agents: [builtInAgent("codex", { status: "setup_required" })],
      recoveryActions: recoveryActions({ onRetry }),
    });

    act(() => {
      buttonByText(view.root, "Try again").props.onClick();
    });

    expect(onRetry).toHaveBeenCalledWith("codex");
  });

  it("offers Node.js recovery when Settings receives the structured setup reason", () => {
    const onOpenExternal = vi.fn();
    const view = renderAgentSettings({
      agents: [{
        ...builtInAgent("codex", { status: "setup_required" }),
        setup_reason: "nodeJsRequired",
      }],
      recoveryActions: recoveryActions({ onOpenExternal }),
    });

    act(() => {
      buttonByText(view.root, "Install Node.js").props.onClick();
    });

    expect(onOpenExternal).toHaveBeenCalledWith("https://nodejs.org/en/download");
  });

  it("keeps setup status copy for Agents that need setup", () => {
    const view = renderAgentSettings({
      agents: [builtInAgent("codex", { status: "setup_required" })],
      recoveryActions: recoveryActions(),
    });

    expect(textContent(view.root)).toContain("Setup is incomplete.");
  });

  it("opens a focused chooser with every ACP authentication method in advertised order", () => {
    const onAuthenticate = vi.fn();
    const view = renderAgentSettings({
      agents: [builtInAgent("codex", {
        auth_methods: [
          { id: "browser", label: "Browser login", kind: "agent" },
          {
            id: "api-key",
            label: "API key",
            kind: "env_var",
            link: "https://example.com/keys",
            variables: [
              { name: "API_KEY", label: "API key", secret: true, optional: false },
              { name: "ENDPOINT", label: "Endpoint", secret: false, optional: true },
            ],
          },
          { id: "terminal", label: "Terminal login", kind: "terminal", terminal_args: ["login"] },
        ],
      })],
      onAuthenticate,
    });

    expect(textContent(view.root)).not.toContain("Browser login");
    expect(view.root.findAllByProps({ "aria-label": "API key" })).toHaveLength(0);

    act(() => {
      buttonByText(view.root, "Manage").props.onClick();
    });
    const methods = view.root.findAllByProps({ className: "agent-sign-in-method" });
    expect(methods.map(textContent)).toEqual(expect.arrayContaining([
      expect.stringContaining("Browser login"),
      expect.stringContaining("API key"),
      expect.stringContaining("Terminal login"),
    ]));
    expect(methods.map(textContent).join(" ")).toMatch(/Browser login.*API key.*Terminal login/);
    expect(view.root.findAllByProps({ "aria-label": "API key" })).toHaveLength(0);

    act(() => {
      buttonByText(view.root, "Browser login").props.onClick();
    });
    expect(onAuthenticate).toHaveBeenCalledWith("codex", "browser");
  });

  it("replaces the chooser with the selected API key step", () => {
    const onAuthenticate = vi.fn();
    const view = renderAgentSettings({
      agents: [builtInAgent("codex", {
        auth_methods: [{
          id: "api-key",
          label: "API key",
          kind: "env_var",
          variables: [{ name: "API_KEY", label: "API key", secret: true, optional: false }],
        }],
      })],
      onAuthenticate,
    });

    act(() => {
      buttonByText(view.root, "Manage").props.onClick();
    });
    act(() => {
      buttonByText(view.root, "API key").props.onClick();
    });

    expect(textContent(view.root)).toContain("Enter your API key");
    expect(buttonByText(view.root, "Back")).toBeTruthy();
    expect(view.root.findByProps({ "aria-label": "API key" }).props.type).toBe("password");
    expect(buttonByText(view.root, "Save").props.disabled).toBe(true);

    act(() => {
      view.root.findByProps({ "aria-label": "API key" }).props.onChange({ currentTarget: { value: "sk-test" } });
    });
    act(() => {
      view.root.findByProps({ className: "agent-page-surface agent-sign-in-value-step" }).props.onSubmit({ preventDefault: vi.fn() });
    });

    expect(onAuthenticate).toHaveBeenCalledWith("codex", "api-key", { API_KEY: "sk-test" });
  });

  it("collapses to the awaiting-user step with the App Server URL and hint", () => {
    const view = renderAgentSettings({
      agents: [builtInAgent("codex", {
        status: "authenticating",
        auth_methods: [
          { id: "chat-gpt", label: "ChatGPT", kind: "agent" },
          { id: "api-key", label: "API key", kind: "env_var", variables: [{ name: "KEY", secret: true, optional: false }] },
        ],
        sign_in: {
          method_id: "chat-gpt",
          phase: "awaiting_user",
          url: "https://auth.openai.com/device",
          hint: "Sign in to ChatGPT and enter this code: ABCD-EFGH",
        },
      })],
      onCancelAuthentication: vi.fn(),
    });

    const openUrl = view.root.findByType("a");
    expect(openUrl.props.href).toBe("https://auth.openai.com/device");
    expect(openUrl.props.target).toBe("_blank");
    expect(textContent(openUrl)).toContain("Open sign-in page");
    expect(textContent(view.root)).toContain("Sign in to ChatGPT and enter this code: ABCD-EFGH");
    expect(textContent(view.root)).toContain("https://auth.openai.com/device");
    expect(textContent(view.root)).toContain("Cancel sign-in");
    // Other methods step aside while one flow runs.
    expect(buttonsByText(view.root, "Choose method")).toHaveLength(0);
    expect(view.root.findAllByProps({ "aria-label": "KEY" })).toHaveLength(0);
  });

  it("shows a starting step that can be cancelled before the Agent answers", () => {
    const onCancelAuthentication = vi.fn();
    const view = renderAgentSettings({
      agents: [builtInAgent("codex", {
        status: "authenticating",
        auth_methods: [{ id: "chat-gpt", label: "ChatGPT", kind: "agent" }],
        sign_in: { method_id: "chat-gpt", phase: "starting" },
      })],
      onCancelAuthentication,
    });

    expect(textContent(view.root)).toContain("Starting ChatGPT");
    act(() => {
      buttonByText(view.root, "Cancel sign-in").props.onClick();
    });
    expect(onCancelAuthentication).toHaveBeenCalledWith("codex");
  });

  it("offers retry or another method after App Server reports a failed flow", () => {
    const onAuthenticate = vi.fn();
    const onCancelAuthentication = vi.fn();
    const view = renderAgentSettings({
      agents: [builtInAgent("codex", {
        status: "auth_required",
        auth_methods: [{ id: "chat-gpt", label: "ChatGPT", kind: "agent" }],
        sign_in: { method_id: "chat-gpt", phase: "failed", failure: "Codex could not sign in with ChatGPT." },
      })],
      onAuthenticate,
      onCancelAuthentication,
    });

    expect(textContent(view.root)).toContain("Codex could not sign in with ChatGPT.");
    act(() => {
      buttonByText(view.root, "Try again").props.onClick();
    });
    expect(onAuthenticate).toHaveBeenCalledWith("codex", "chat-gpt");

    act(() => {
      buttonByText(view.root, "Choose another method").props.onClick();
    });
    expect(onCancelAuthentication).toHaveBeenCalledWith("codex");
  });

  it("does not reserve a browser window when ChatGPT sign-in starts", () => {
    const reserveExternalOpen = vi.fn();
    const onAuthenticate = vi.fn();
    installFrontendShell({ recovery: { openExternal: vi.fn(), reserveExternalOpen } } as never);
    const view = renderAgentSettings({
      agents: [builtInAgent("codex", {
        status: "connected",
        auth_methods: [{ id: "chat-gpt", label: "ChatGPT", kind: "agent" }],
      })],
      onAuthenticate,
    });

    act(() => {
      buttonByText(view.root, "Manage").props.onClick();
    });
    act(() => {
      buttonByText(view.root, "ChatGPT").props.onClick();
    });

    expect(reserveExternalOpen).not.toHaveBeenCalled();
    expect(onAuthenticate).toHaveBeenCalledWith("codex", "chat-gpt");
  });

  it("hides Cancel sign-in when no flow is running", () => {
    const view = renderAgentSettings({
      agents: [builtInAgent("codex", {
        status: "auth_required",
        auth_methods: [{ id: "chat-gpt", label: "ChatGPT", kind: "agent" }],
      })],
      onCancelAuthentication: vi.fn(),
    });

    expect(textContent(view.root)).not.toContain("Cancel sign-in");
  });

  it("presents connected Agent authentication choices without repeating the state", () => {
    const view = renderAgentSettings({
      agents: [builtInAgent("codex", {
        status: "connected",
        auth_methods: [{ id: "chat-gpt", label: "ChatGPT", kind: "agent" }],
      })],
    });

    expect(textContent(view.root)).toContain("AuthenticationManage");
    expect(view.root.findAllByProps({ "aria-label": "Authentication" })).toHaveLength(1);
    expect(textContent(view.root)).not.toContain("Sign in");
    expect(textContent(view.root)).not.toContain("Codex is connected");
    expect(textContent(view.root)).not.toContain("Authentication available");
    expect(textContent(view.root)).not.toContain("offered by Codex");
  });

  it("states explicitly when ACP reports that sign-in is required", () => {
    const view = renderAgentSettings({
      agents: [builtInAgent("codex", {
        status: "auth_required",
        auth_methods: [{ id: "chat-gpt", label: "ChatGPT", kind: "agent" }],
      })],
    });

    expect(textContent(view.root)).toContain("Sign in requiredChoose method");
    expect(view.root.findAllByProps({ "aria-label": "Sign in" })).toHaveLength(1);
    expect(textContent(view.root)).not.toContain("Authentication required");
    expect(textContent(view.root)).not.toContain("Sign-in is required to continue.");
  });

  it("explains launching instead of leaving Codex on a blank connection page", () => {
    const view = renderAgentSettings({
      agents: [builtInAgent("codex", { status: "launching" })],
    });

    expect(textContent(view.root)).toContain("Starting");
    expect(textContent(view.root)).toContain("Starting the Agent process. Sign-in choices appear when it answers.");
    expect(textContent(view.root)).not.toContain("Action required");
  });

  it("writes the selected Agent into the Settings route so refresh stays on that detail", () => {
    const replaceSettingsAgent = vi.fn();
    installFrontendShell({ navigation: { replaceSettingsAgent } } as never);
    const view = renderAgentSettings({
      agents: [builtInAgent("codex"), builtInAgent("opencode", { label: "OpenCode" })],
      openFirst: false,
    });

    act(() => {
      view.root.findAllByProps({ className: "agent-catalog-row" })
        .find((button) => textContent(button).includes("Codex"))!
        .props.onClick();
    });
    expect(replaceSettingsAgent).toHaveBeenCalledWith("codex");

    act(() => {
      view.root.findByProps({ "aria-label": "Back to Agents" }).props.onClick();
    });
    expect(replaceSettingsAgent).toHaveBeenLastCalledWith();
  });

  it("detects when a live Agent collection leaves launching", () => {
    expect(agentLeftLaunching(
      [{ id: "codex", status: "launching" }],
      [{ id: "codex", status: "auth_required" }],
    )).toBe(true);
    expect(agentLeftLaunching(
      [{ id: "codex", status: "launching" }],
      [{ id: "codex", status: "launching" }],
    )).toBe(false);
  });

  it("asks for confirmation while a terminal sign-in awaits the user", () => {
    const onAuthenticate = vi.fn();
    const agent = builtInAgent("codex", {
      status: "authenticating",
      sign_in: { method_id: "terminal-login", phase: "awaiting_terminal" },
      auth_methods: [
        { id: "terminal-login", label: "Sign in in terminal", kind: "terminal" },
        { id: "browser-login", label: "Sign in with browser", kind: "agent" },
      ],
    });

    const view = renderAgentSettings({ agents: [agent], onAuthenticate });

    expect(buttonsByText(view.root, "Choose method")).toHaveLength(0);
    act(() => {
      buttonByText(view.root, "I've finished signing in").props.onClick();
    });
    expect(onAuthenticate).toHaveBeenCalledWith("codex", "terminal-login");
  });

  it("shows a useful empty Agent catalog", () => {
    const view = renderAgentSettings({ agents: [] });

    expect(textContent(view.root)).toContain("No agents configured");
    expect(buttonByText(view.root, "Add agent")).toBeTruthy();
  });
});

function renderAgentSettings({
  agents,
  onCreateCustomAgent = vi.fn(),
  onDeleteCustomAgent = vi.fn(),
  onReplaceCustomAgent = vi.fn(),
  onSetAgentEnabled = vi.fn(),
  onUpdateCustomAgentMetadata = vi.fn(),
  onAuthenticate = vi.fn(),
  onCancelAuthentication,
  openFirst = true,
  recoveryActions,
}: {
  agents: AgentSettingsRecord[];
  onCreateCustomAgent?: Parameters<typeof AgentSettingsTab>[0]["onCreateCustomAgent"];
  onDeleteCustomAgent?: (agentId: string) => void;
  onReplaceCustomAgent?: Parameters<typeof AgentSettingsTab>[0]["onReplaceCustomAgent"];
  onSetAgentEnabled?: (agentId: string, enabled: boolean) => void;
  onUpdateCustomAgentMetadata?: Parameters<typeof AgentSettingsTab>[0]["onUpdateCustomAgentMetadata"];
  onAuthenticate?: Parameters<typeof AgentSettingsTab>[0]["onAuthenticate"];
  onCancelAuthentication?: Parameters<typeof AgentSettingsTab>[0]["onCancelAuthentication"];
  openFirst?: boolean;
  recoveryActions?: AgentRecoveryActions;
}) {
  let view: ReactTestRenderer | undefined;
  act(() => {
    view = create(
      <AgentSettingsTab
        agents={agents}
        onAuthenticate={onAuthenticate}
        onCancelAuthentication={onCancelAuthentication}
        onCreateCustomAgent={onCreateCustomAgent}
        onDeleteCustomAgent={onDeleteCustomAgent}
        onReplaceCustomAgent={onReplaceCustomAgent}
        onSetAgentEnabled={onSetAgentEnabled}
        onUpdateCustomAgentMetadata={onUpdateCustomAgentMetadata}
        recoveryActions={recoveryActions}
      />,
    );
  });
  if (openFirst && agents.length) {
    act(() => {
      view!.root.findAllByProps({ className: "agent-catalog-row" })
        .find((button) => textContent(button).includes(agents[0].label))!
        .props.onClick();
    });
  }
  return view!;
}

function recoveryActions(overrides: Partial<AgentRecoveryActions> = {}): AgentRecoveryActions {
  return {
    onOpenAgentSettings: vi.fn(),
    onOpenExternal: vi.fn(),
    onRetry: vi.fn(async () => true),
    ...overrides,
  };
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
