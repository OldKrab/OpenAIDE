import { act, create } from "react-test-renderer";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentSettingsRecord } from "@openaide/app-shell-contracts";
import { shouldConsumeAgentDeleteAck, shouldConsumeAgentSaveAck } from "./AgentSettingsTab";
import { compactPathForSettings } from "./GeneralSettingsTab";
import { SettingsView } from "./SettingsView";

beforeEach(() => {
  (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

describe("SettingsView custom Agent acknowledgements", () => {
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

    expect(tree.root.findAllByType("strong").some((item) => item.children.includes("Local Agent"))).toBe(true);
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
          activeTab: "common",
          loading: false,
          runtimeSettings: {
            developer: { acp_trace: { enabled: true, directory: "/runtime/traces" } },
          },
        }}
      />,
    );

    expect(tree.root.findAllByType("input").some((input) => input.props["aria-label"] === "Search settings")).toBe(true);
    expect(tree.root.findAllByType("input").some((input) => input.props["aria-label"] === "Enter sends message")).toBe(true);
    expect(tree.root.findAllByType("input").some((input) => input.props["aria-label"] === "ACP logs")).toBe(false);
    expect(tree.root.findAllByType("code").some((code) => code.props.title === "/runtime/traces")).toBe(false);
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
          activeTab: "common",
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
    expect(tree.root.findAllByType("strong").some((item) => item.children.includes("General"))).toBe(true);
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

    const tabLabels = tree.root.findAllByProps({ role: "tab" }).map((tab) => tab.children.join(""));
    expect(tabLabels).toEqual(["Agents", "General"]);
    expect(tree.root.findByProps({ role: "tabpanel" }).props["aria-labelledby"]).toBe("settings-tab-agents");
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
    expect(tabs.map((tab) => Boolean(tab.props.autoFocus))).toEqual([false, false, false, true]);
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

    const tabLabels = tree.root.findAllByProps({ role: "tab" }).map((tab) => tab.children.join(""));
    expect(tabLabels).toEqual(["Agents", "MCP", "Skills", "General"]);
    expect(tree.root.findByProps({ role: "tabpanel" }).props["aria-labelledby"]).toBe("settings-tab-mcp");
    expect(tree.root.findAllByType("strong").some((item) => item.children.includes("MCP discovery unavailable"))).toBe(true);
    expect(tree.root.findAllByType("span").some((item) =>
      item.children.includes("OpenAIDE cannot currently enumerate MCP servers from the App Server.")
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
