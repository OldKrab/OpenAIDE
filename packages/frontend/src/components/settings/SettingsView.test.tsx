import { act, create } from "react-test-renderer";
import type { ReactTestInstance } from "react-test-renderer";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentSettingsRecord } from "@openaide/app-shell-contracts";
import { shouldConsumeAgentDeleteAck, shouldConsumeAgentSaveAck } from "./AgentSettingsTab";
import { compactPathForSettings, DataSupportSettingsTab, GeneralSettingsTab } from "./GeneralSettingsTab";
import { SettingsView } from "./SettingsView";

beforeEach(() => {
  (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

afterEach(() => vi.unstubAllGlobals());

describe("SettingsView custom Agent acknowledgements", () => {
  it("opens the Settings index first on a narrow viewport", () => {
    vi.stubGlobal("window", {
      cancelAnimationFrame: vi.fn(),
      localStorage: { getItem: vi.fn(() => null), setItem: vi.fn() },
      matchMedia: vi.fn((query: string) => ({ matches: query === "(max-width: 760px)" })),
      requestAnimationFrame: vi.fn(() => 1),
    });

    const tree = renderSettingsView();

    expect(tree.root.findByProps({ className: "settings-mobile-index open" })).toBeTruthy();
    expect(tree.root.findByProps({ className: "settings-content mobile-index-open" })).toBeTruthy();
  });

  it("hides desktop composer shortcut settings on a mobile pointer", () => {
    vi.stubGlobal("window", {
      matchMedia: vi.fn(() => ({ matches: true })),
    });
    const tree = render(
      <GeneralSettingsTab
        onSetComposerSubmitShortcut={() => undefined}
        preferences={{ composer_submit_shortcut: "enter" }}
      />,
    );

    expect(tree.root.findAllByType("input").some((input) => input.props["aria-label"] === "Enter sends message")).toBe(false);
    expect(tree.root.findAllByType("strong").some((item) => item.children.includes("New line shortcut"))).toBe(false);
  });

  it("lets a shell expose and update its app theme", () => {
    const setTheme = vi.fn();
    const tree = render(
      <GeneralSettingsTab
        appearance={{ theme: () => "system", setTheme }}
        onSetComposerSubmitShortcut={() => undefined}
        preferences={{ composer_submit_shortcut: "enter" }}
      />,
    );

    const choices = tree.root.findAll((node) => node.props.role === "radio");
    expect(choices).toHaveLength(3);
    expect(choices.map((choice) => choice.props["aria-checked"])).toEqual([true, false, false]);

    act(() => choices[2].props.onClick());

    expect(setTheme).toHaveBeenCalledWith("dark");
    expect(tree.root.findAll((node) => node.props.role === "radio").map((choice) => choice.props["aria-checked"]))
      .toEqual([false, false, true]);
  });

  it("omits theme controls when the host owns appearance", () => {
    const tree = render(
      <GeneralSettingsTab
        onSetComposerSubmitShortcut={() => undefined}
        preferences={{ composer_submit_shortcut: "enter" }}
      />,
    );

    expect(tree.root.findAll((node) => node.props.role === "radio")).toHaveLength(0);
  });

  it("requires explicit confirmation before resetting Task history", async () => {
    const resetTaskHistory = vi.fn(async () => undefined);
    const tree = render(
      <DataSupportSettingsTab
        onResetTaskHistory={resetTaskHistory}
        onSetAcpTrace={() => undefined}
      />,
    );

    act(() => {
      tree.root.findByProps({ "aria-label": "Reset task history" }).props.onClick();
    });

    expect(resetTaskHistory).not.toHaveBeenCalled();
    expect(tree.root.findByProps({ "aria-label": "Reset task history confirmation" })).toBeTruthy();
    expect(tree.root.findAllByType("p").some((item) => item.children.join("").includes("Agent-owned sessions"))).toBe(true);

    await act(async () => {
      await tree.root.findByProps({ "aria-label": "Confirm reset task history" }).props.onClick();
    });

    expect(resetTaskHistory).toHaveBeenCalledOnce();
    expect(tree.root.findAllByProps({ "aria-label": "Reset task history confirmation" })).toHaveLength(0);
  });

  it("consumes save acknowledgements only for the draft that initiated the save", () => {
    expect(
      shouldConsumeAgentSaveAck({
        hasDraft: true,
        pendingSaveAgentId: "custom.local",
        savedAgentId: "custom.local",
      }),
    ).toBe(true);

    expect(
      shouldConsumeAgentSaveAck({
        hasDraft: true,
        pendingSaveAgentId: "custom.local",
        savedAgentId: "other.agent",
      }),
    ).toBe(false);

    expect(
      shouldConsumeAgentSaveAck({
        hasDraft: true,
        pendingSaveAgentId: "custom.local",
        removedAgentId: "custom.local",
        savedAgentId: "custom.replacement",
      }),
    ).toBe(true);

    expect(
      shouldConsumeAgentSaveAck({
        hasDraft: true,
        pendingSaveAgentId: "__new__",
        savedAgentId: "created.agent",
      }),
    ).toBe(true);

    expect(
      shouldConsumeAgentSaveAck({
        hasDraft: false,
        pendingSaveAgentId: "custom.local",
        savedAgentId: "custom.local",
      }),
    ).toBe(false);
  });

  it("consumes delete acknowledgements only for the pending Agent", () => {
    expect(
      shouldConsumeAgentDeleteAck({
        pendingDeleteAgentId: "custom.local",
        deletedAgentId: "custom.local",
      }),
    ).toBe(true);

    expect(
      shouldConsumeAgentDeleteAck({
        pendingDeleteAgentId: "custom.local",
        deletedAgentId: "other.agent",
      }),
    ).toBe(false);
  });

  it("renders Backend Agent details without a full Settings snapshot", () => {
    const tree = render(
      <SettingsView
        onAuthenticate={() => undefined}
        onCreateCustomAgent={() => undefined}
        onDeleteCustomAgent={() => undefined}
        onRefresh={() => undefined}
        onReplaceCustomAgent={() => undefined}
        onSelectTab={() => undefined}
        onSetAcpTrace={() => undefined}
        onSetAgentEnabled={() => undefined}
        onSetComposerSubmitShortcut={() => undefined}
        onUpdateCustomAgentMetadata={() => undefined}
        onUnlockDeveloperSettings={() => undefined}
        preferences={{ composer_submit_shortcut: "mod_enter" }}
        state={{
          activeTab: "agents",
          loading: false,
          agentDetails: [agent("custom.local")],
        }}
      />,
    );

    const agentRow = tree.root.findByProps({ className: "agent-catalog-row" });
    expect(agentRow.findAllByType("strong").some((item) => item.children.includes("Local Agent"))).toBe(true);
    act(() => agentRow.props.onClick());
    expect(tree.root.findAllByType("input").some((input) => input.props.value === "local-agent --stdio")).toBe(true);
  });

  it("keeps Backend runtime developer settings hidden until developer controls are unlocked", () => {
    const tree = render(
      <SettingsView
        onAuthenticate={() => undefined}
        onCreateCustomAgent={() => undefined}
        onDeleteCustomAgent={() => undefined}
        onRefresh={() => undefined}
        onReplaceCustomAgent={() => undefined}
        onSelectTab={() => undefined}
        onSetAcpTrace={() => undefined}
        onSetAgentEnabled={() => undefined}
        onSetComposerSubmitShortcut={() => undefined}
        onUpdateCustomAgentMetadata={() => undefined}
        onUnlockDeveloperSettings={() => undefined}
        preferences={{ composer_submit_shortcut: "mod_enter" }}
        state={{
          activeTab: "data",
          loading: false,
          runtimeSettings: {
            developer: { acp_trace: { enabled: true, directory: "/runtime/traces" } },
          },
        }}
      />,
    );

    expect(tree.root.findAllByType("input").some((input) => input.props["aria-label"] === "Search settings")).toBe(false);
    expect(tree.root.findAllByType("input").some((input) => input.props["aria-label"] === "Send with Enter")).toBe(false);
    expect(tree.root.findAllByType("input").some((input) => input.props["aria-label"] === "ACP logs")).toBe(false);
    expect(tree.root.findAllByType("code").some((code) => code.props.title === "/runtime/traces")).toBe(false);
  });

  it("restores the persisted developer unlock supplied by the App Shell", () => {
    const tree = render(
      <SettingsView
        developerSettingsUnlocked
        onAuthenticate={() => undefined}
        onCreateCustomAgent={() => undefined}
        onDeleteCustomAgent={() => undefined}
        onRefresh={() => undefined}
        onReplaceCustomAgent={() => undefined}
        onSelectTab={() => undefined}
        onSetAcpTrace={() => undefined}
        onSetAgentEnabled={() => undefined}
        onSetComposerSubmitShortcut={() => undefined}
        onUpdateCustomAgentMetadata={() => undefined}
        onUnlockDeveloperSettings={() => undefined}
        preferences={{ composer_submit_shortcut: "mod_enter" }}
        state={{
          activeTab: "data",
          loading: false,
          runtimeSettings: {
            developer: { acp_trace: { enabled: true, directory: "/runtime/traces" } },
          },
        }}
      />,
    );

    expect(tree.root.findAllByType("input").some((input) => input.props["aria-label"] === "ACP logs")).toBe(true);
  });

  it("renders browser-local desktop notification state and forwards opt-in", () => {
    const onSetDesktopNotifications = vi.fn();
    const tree = render(
      <SettingsView
        desktopNotifications={{ status: "blocked" }}
        onAuthenticate={() => undefined}
        onCreateCustomAgent={() => undefined}
        onDeleteCustomAgent={() => undefined}
        onRefresh={() => undefined}
        onReplaceCustomAgent={() => undefined}
        onSelectTab={() => undefined}
        onSetAcpTrace={() => undefined}
        onSetAgentEnabled={() => undefined}
        onSetComposerSubmitShortcut={() => undefined}
        onSetDesktopNotifications={onSetDesktopNotifications}
        onUpdateCustomAgentMetadata={() => undefined}
        onUnlockDeveloperSettings={() => undefined}
        preferences={{ composer_submit_shortcut: "mod_enter" }}
        state={{ activeTab: "common", loading: false }}
      />,
    );

    const toggle = tree.root.findByProps({ "aria-label": "Desktop notifications", type: "checkbox" });
    expect(toggle.props.checked).toBe(true);
    expect(JSON.stringify(tree.toJSON())).toContain("Blocked by the browser or OS");
    act(() => toggle.props.onChange({ currentTarget: { checked: false } }));
    expect(onSetDesktopNotifications).toHaveBeenCalledWith(false);
  });

  it("reveals developer runtime settings after the hidden unlock gesture", () => {
    const onUnlockDeveloperSettings = vi.fn();
    const tree = render(
      <SettingsView
        onAuthenticate={() => undefined}
        onCreateCustomAgent={() => undefined}
        onDeleteCustomAgent={() => undefined}
        onRefresh={() => undefined}
        onReplaceCustomAgent={() => undefined}
        onSelectTab={() => undefined}
        onSetAcpTrace={() => undefined}
        onSetAgentEnabled={() => undefined}
        onSetComposerSubmitShortcut={() => undefined}
        onUpdateCustomAgentMetadata={() => undefined}
        onUnlockDeveloperSettings={onUnlockDeveloperSettings}
        preferences={{ composer_submit_shortcut: "mod_enter" }}
        state={{
          activeTab: "data",
          loading: false,
          runtimeSettings: {
            developer: { acp_trace: { enabled: true, directory: "/runtime/traces" } },
          },
        }}
      />,
    );

    const title = tree.root.findByProps({ className: "settings-title-button" });
    for (let index = 0; index < 7; index += 1) {
      act(() => title.props.onClick());
    }

    expect(onUnlockDeveloperSettings).toHaveBeenCalledTimes(1);
    expect(tree.root.findAllByType("input").some((input) => input.props["aria-label"] === "ACP logs")).toBe(true);
    const traceDirectory = tree.root.findAllByType("code").find((code) => code.props.title === "/runtime/traces");
    expect(traceDirectory?.children).toEqual(["/runtime/traces"]);
  });

  it("keeps the active General panel visible while settings refresh", () => {
    const tree = render(
      <SettingsView
        onAuthenticate={() => undefined}
        onCreateCustomAgent={() => undefined}
        onDeleteCustomAgent={() => undefined}
        onRefresh={() => undefined}
        onReplaceCustomAgent={() => undefined}
        onSelectTab={() => undefined}
        onSetAcpTrace={() => undefined}
        onSetAgentEnabled={() => undefined}
        onSetComposerSubmitShortcut={() => undefined}
        onUpdateCustomAgentMetadata={() => undefined}
        onUnlockDeveloperSettings={() => undefined}
        preferences={{ composer_submit_shortcut: "mod_enter" }}
        state={{
          activeTab: "common",
          availableTabs: ["agents", "mcp", "skills", "common"],
          loading: true,
          runtimeSettings: {
            developer: { acp_trace: { enabled: false, directory: "/runtime/traces" } },
          },
        }}
      />,
    );

    expect(tree.root.findByProps({ role: "tabpanel" }).props["aria-labelledby"]).toBe("settings-tab-common");
    expect(tree.root.findAllByType("h1").some((item) => item.children.includes("General"))).toBe(true);
  });

  it("compacts long local paths while preserving the full title", () => {
    expect(compactPathForSettings("/Users/developer/src/OpenAIDE/.openaide-web-dev-single/state/diagnostics/acp-traces")).toBe(
      ".../state/diagnostics/acp-traces",
    );
    expect(compactPathForSettings("/runtime/traces")).toBe("/runtime/traces");
  });

  it("hides Settings sections that do not have App Server projections yet", () => {
    const tree = render(
      <SettingsView
        onAuthenticate={() => undefined}
        onCreateCustomAgent={() => undefined}
        onDeleteCustomAgent={() => undefined}
        onRefresh={() => undefined}
        onReplaceCustomAgent={() => undefined}
        onSelectTab={() => undefined}
        onSetAcpTrace={() => undefined}
        onSetAgentEnabled={() => undefined}
        onSetComposerSubmitShortcut={() => undefined}
        onUpdateCustomAgentMetadata={() => undefined}
        onUnlockDeveloperSettings={() => undefined}
        preferences={{ composer_submit_shortcut: "mod_enter" }}
        state={{
          activeTab: "skills",
          loading: false,
          agentDetails: [agent("custom.local")],
        }}
      />,
    );

    const tabLabels = tree.root.findAllByProps({ role: "tab" }).map(settingsTabLabel);
    expect(tabLabels).toEqual(["General", "Data & support", "Agents", "Worktrees"]);
    expect(tree.root.findByProps({ role: "tabpanel" }).props["aria-labelledby"]).toBe("settings-tab-common");
  });

  it("finds a setting by control name and opens its page", () => {
    const onSelectTab = vi.fn();
    const tree = render(
      <SettingsView
        onAuthenticate={() => undefined}
        onCreateCustomAgent={() => undefined}
        onDeleteCustomAgent={() => undefined}
        onRefresh={() => undefined}
        onReplaceCustomAgent={() => undefined}
        onResetTaskHistory={async () => undefined}
        onSelectTab={onSelectTab}
        onSetAcpTrace={() => undefined}
        onSetAgentEnabled={() => undefined}
        onSetComposerSubmitShortcut={() => undefined}
        onUpdateCustomAgentMetadata={() => undefined}
        onUnlockDeveloperSettings={() => undefined}
        preferences={{ composer_submit_shortcut: "mod_enter" }}
        state={{ activeTab: "common", loading: false }}
      />,
    );

    const search = tree.root.findAllByProps({ "aria-label": "Search Settings" })[0]!;
    act(() => search.props.onChange({ currentTarget: { value: "reset" } }));
    const result = tree.root.findAllByType("button").find((button) =>
      button.findAllByType("strong").some((label) => label.children.includes("Reset task history"))
    );
    act(() => result?.props.onClick());

    expect(onSelectTab).toHaveBeenCalledWith("data");
  });

  it("keeps Desktop settings out of shells without native Desktop capabilities", () => {
    const tree = render(
      <SettingsView
        onAuthenticate={() => undefined}
        onCreateCustomAgent={() => undefined}
        onDeleteCustomAgent={() => undefined}
        onRefresh={() => undefined}
        onReplaceCustomAgent={() => undefined}
        onSelectTab={() => undefined}
        onSetAcpTrace={() => undefined}
        onSetAgentEnabled={() => undefined}
        onSetComposerSubmitShortcut={() => undefined}
        onUpdateCustomAgentMetadata={() => undefined}
        onUnlockDeveloperSettings={() => undefined}
        preferences={{ composer_submit_shortcut: "mod_enter" }}
        state={{ activeTab: "common", availableTabs: ["common", "desktop", "data"], loading: false }}
      />,
    );

    expect(tree.root.findAllByProps({ role: "tab" }).map(settingsTabLabel)).toEqual([
      "General",
      "Data & support",
      "Worktrees",
    ]);
  });

  it("moves focus to the active tab when Settings opens", () => {
    const tree = render(
      <SettingsView
        onAuthenticate={() => undefined}
        onCreateCustomAgent={() => undefined}
        onDeleteCustomAgent={() => undefined}
        onRefresh={() => undefined}
        onReplaceCustomAgent={() => undefined}
        onSelectTab={() => undefined}
        onSetAcpTrace={() => undefined}
        onSetAgentEnabled={() => undefined}
        onSetComposerSubmitShortcut={() => undefined}
        onUpdateCustomAgentMetadata={() => undefined}
        onUnlockDeveloperSettings={() => undefined}
        preferences={{ composer_submit_shortcut: "mod_enter" }}
        state={{
          activeTab: "common",
          availableTabs: ["agents", "mcp", "skills", "common"],
          loading: false,
          agentDetails: [],
        }}
      />,
    );

    const tabs = tree.root.findAllByProps({ role: "tab" });
    expect(tabs.map((tab) => Boolean(tab.props.autoFocus))).toEqual([true, false, false, false, false]);
  });

  it("returns to the app from the Settings sidebar", () => {
    const onBackToApp = vi.fn();
    const tree = render(
      <SettingsView
        onAuthenticate={() => undefined}
        onBackToApp={onBackToApp}
        onCreateCustomAgent={() => undefined}
        onDeleteCustomAgent={() => undefined}
        onRefresh={() => undefined}
        onReplaceCustomAgent={() => undefined}
        onSelectTab={() => undefined}
        onSetAcpTrace={() => undefined}
        onSetAgentEnabled={() => undefined}
        onSetComposerSubmitShortcut={() => undefined}
        onUpdateCustomAgentMetadata={() => undefined}
        onUnlockDeveloperSettings={() => undefined}
        preferences={{ composer_submit_shortcut: "mod_enter" }}
        state={{ activeTab: "common", loading: false }}
      />,
    );

    act(() => tree.root.findAllByProps({ "aria-label": "Close settings" })[0]?.props.onClick());

    expect(onBackToApp).toHaveBeenCalledOnce();
  });

  it("explains when App Server MCP discovery is unavailable instead of claiming the list is empty", () => {
    const tree = render(
      <SettingsView
        onAuthenticate={() => undefined}
        onCreateCustomAgent={() => undefined}
        onDeleteCustomAgent={() => undefined}
        onRefresh={() => undefined}
        onReplaceCustomAgent={() => undefined}
        onSelectTab={() => undefined}
        onSetAcpTrace={() => undefined}
        onSetAgentEnabled={() => undefined}
        onSetComposerSubmitShortcut={() => undefined}
        onUpdateCustomAgentMetadata={() => undefined}
        onUnlockDeveloperSettings={() => undefined}
        preferences={{ composer_submit_shortcut: "mod_enter" }}
        state={{
          activeTab: "mcp",
          availableTabs: ["agents", "mcp", "skills", "common"],
          loading: false,
          mcpServersAvailability: "unavailable",
          mcpServers: [],
          skills: [],
        }}
      />,
    );

    const tabLabels = tree.root.findAllByProps({ role: "tab" }).map(settingsTabLabel);
    expect(tabLabels).toEqual(["General", "Agents", "MCP Servers", "Skills", "Worktrees"]);
    expect(tree.root.findByProps({ role: "tabpanel" }).props["aria-labelledby"]).toBe("settings-tab-mcp");
    expect(tree.root.findAllByType("strong").some((item) => item.children.includes("MCP settings unavailable"))).toBe(true);
    expect(tree.root.findAllByType("span").some((item) =>
      item.children.includes("The App Server could not read MCP configuration.")
    )).toBe(true);
    expect(tree.root.findAllByType("strong").some((item) => item.children.includes("No MCP servers"))).toBe(false);
  });

  it("explains when App Server Skills discovery is unavailable instead of claiming the list is empty", () => {
    const tree = render(
      <SettingsView
        onAuthenticate={() => undefined}
        onCreateCustomAgent={() => undefined}
        onDeleteCustomAgent={() => undefined}
        onRefresh={() => undefined}
        onReplaceCustomAgent={() => undefined}
        onSelectTab={() => undefined}
        onSetAcpTrace={() => undefined}
        onSetAgentEnabled={() => undefined}
        onSetComposerSubmitShortcut={() => undefined}
        onUpdateCustomAgentMetadata={() => undefined}
        onUnlockDeveloperSettings={() => undefined}
        preferences={{ composer_submit_shortcut: "mod_enter" }}
        state={{
          activeTab: "skills",
          availableTabs: ["agents", "mcp", "skills", "common"],
          loading: false,
          skillsAvailability: "unavailable",
          skills: [],
        }}
      />,
    );

    expect(tree.root.findAllByType("strong").some((item) => item.children.includes("Skills discovery unavailable"))).toBe(true);
    expect(tree.root.findAllByType("span").some((item) =>
      item.children.includes("OpenAIDE cannot currently enumerate installed skills from the App Server.")
    )).toBe(true);
    expect(tree.root.findAllByType("strong").some((item) => item.children.includes("No skills"))).toBe(false);
  });

  it("renders Skills loading state independently from Agent settings loading", () => {
    const tree = render(
      <SettingsView
        onAuthenticate={() => undefined}
        onCreateCustomAgent={() => undefined}
        onDeleteCustomAgent={() => undefined}
        onRefresh={() => undefined}
        onReplaceCustomAgent={() => undefined}
        onSelectTab={() => undefined}
        onSetAcpTrace={() => undefined}
        onSetAgentEnabled={() => undefined}
        onSetComposerSubmitShortcut={() => undefined}
        onUpdateCustomAgentMetadata={() => undefined}
        onUnlockDeveloperSettings={() => undefined}
        preferences={{ composer_submit_shortcut: "mod_enter" }}
        state={{
          activeTab: "skills",
          availableTabs: ["agents", "mcp", "skills", "common"],
          loading: false,
          skillsLoading: true,
        }}
      />,
    );

    expect(tree.root.findByProps({ "aria-label": "Loading settings" })).toBeTruthy();
    expect(tree.root.findAllByType("strong").some((item) => item.children.includes("Loading settings"))).toBe(true);
  });

  it("renders visible loading copy for MCP before projection data arrives", () => {
    const tree = render(
      <SettingsView
        onAuthenticate={() => undefined}
        onCreateCustomAgent={() => undefined}
        onDeleteCustomAgent={() => undefined}
        onRefresh={() => undefined}
        onReplaceCustomAgent={() => undefined}
        onSelectTab={() => undefined}
        onSetAcpTrace={() => undefined}
        onSetAgentEnabled={() => undefined}
        onSetComposerSubmitShortcut={() => undefined}
        onUpdateCustomAgentMetadata={() => undefined}
        onUnlockDeveloperSettings={() => undefined}
        preferences={{ composer_submit_shortcut: "mod_enter" }}
        state={{
          activeTab: "mcp",
          availableTabs: ["agents", "mcp", "skills", "common"],
          loading: false,
          mcpServersLoading: true,
        }}
      />,
    );

    expect(tree.root.findByProps({ role: "tabpanel" }).props["aria-labelledby"]).toBe("settings-tab-mcp");
    expect(tree.root.findAllByType("strong").some((item) => item.children.includes("Loading settings"))).toBe(true);
  });
});

function render(element: React.ReactElement) {
  let tree: ReturnType<typeof create> | undefined;
  act(() => {
    tree = create(element);
  });
  return tree!;
}

function renderSettingsView() {
  return render(
    <SettingsView
      onAuthenticate={() => undefined}
      onCreateCustomAgent={() => undefined}
      onDeleteCustomAgent={() => undefined}
      onRefresh={() => undefined}
      onReplaceCustomAgent={() => undefined}
      onSelectTab={() => undefined}
      onSetAcpTrace={() => undefined}
      onSetAgentEnabled={() => undefined}
      onSetComposerSubmitShortcut={() => undefined}
      onUpdateCustomAgentMetadata={() => undefined}
      onUnlockDeveloperSettings={() => undefined}
      preferences={{ composer_submit_shortcut: "mod_enter" }}
      state={{ activeTab: "common", loading: false }}
    />,
  );
}

function settingsTabLabel(tab: ReactTestInstance) {
  return tab.findByType("span").children.join("");
}

function agent(id: string): AgentSettingsRecord {
  return {
    id,
    label: "Local Agent",
    enabled: true,
    scope: "global",
    source_kind: "custom",
    icon: "terminal",
    transport: "stdio",
    status: "connected",
    launch_label: "local-agent",
    command_line: "local-agent --stdio",
    env: [],
    description: "Custom ACP stdio Agent",
    capabilities: [],
    auth_methods: [],
  };
}
