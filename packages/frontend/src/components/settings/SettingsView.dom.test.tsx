// @vitest-environment jsdom

import { afterEach, expect, it, vi } from "vitest";
import * as frontendShell from "../../services/frontendShell";
import { builtInAgent, button, buttons, click, dialog, field, mountSettings, pageText, settle, type, unmount } from "./settingsTestSupport";

afterEach(() => {
  unmount();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

const sectionNames = () => [...document.querySelectorAll('nav[aria-label="Settings sections"] > button')].map((item) => item.querySelector("span")?.textContent);
const openSection = () => document.querySelector('nav[aria-label="Settings sections"] [aria-current="page"]')?.querySelector("span")?.textContent;

it("lists the sections this client offers in a fixed order and reports the one chosen", async () => {
  const onSelectTab = vi.fn();
  await mountSettings({
    deviceIntents: fakeDeviceIntents(),
    onSelectTab,
    state: { activeTab: "general", availableTabs: ["agents", "mcp", "skills", "general", "troubleshooting"], agentDetails: [], loading: false },
  });

  expect(sectionNames()).toEqual(["General", "Agents", "MCP servers", "Skills", "Remote", "Troubleshooting"]);
  expect(openSection()).toBe("General");

  await click("Troubleshooting");

  expect(onSelectTab).toHaveBeenCalledWith("troubleshooting");
});

it("leaves out sections the App Server does not offer", async () => {
  await mountSettings({ state: { activeTab: "general", availableTabs: ["general", "troubleshooting"], loading: false } });

  expect(sectionNames()).toEqual(["General", "Troubleshooting"]);
});

it("searches every section at once and keeps only what matches", async () => {
  await mountSettings({
    onResetTaskHistory: async () => undefined,
    state: { activeTab: "general", agentDetails: [builtInAgent()], loading: false },
  });

  await type("Search settings", "codex");

  expect(sectionNames()).toEqual(["Agents"]);
  expect(openSection()).toBe("Agents");
  expect(pageText()).toContain("OpenAI coding agent.");
  expect(pageText()).not.toContain("Send with Enter");

  await type("Search settings", "no such setting");

  expect(sectionNames()).toEqual([]);
  expect(pageText()).toContain("Nothing found.");

  await click("Clear search");

  expect(sectionNames()).toEqual(["General", "Agents", "Troubleshooting"]);
});

it("finds a setting by what its explanation says", async () => {
  await mountSettings({ state: { activeTab: "general", agentDetails: [builtInAgent()], loading: false } });

  // "ACP" appears only in the Agents explanation, never in a visible label.
  await type("Search settings", "speaks acp");

  expect(sectionNames()).toEqual(["Agents"]);
});

it("changes the send shortcut from General", async () => {
  const onSetComposerSubmitShortcut = vi.fn();
  await mountSettings({ onSetComposerSubmitShortcut });

  expect(button("Send with Enter").getAttribute("aria-checked")).toBe("false");
  await click("Send with Enter");

  expect(onSetComposerSubmitShortcut).toHaveBeenCalledWith("enter");
});

it("offers a theme choice only when the shell owns appearance", async () => {
  const setTheme = vi.fn();
  await mountSettings({ appearance: { theme: () => "system", setTheme } });

  const choices = [...document.querySelectorAll<HTMLElement>('[role="radiogroup"][aria-label="Theme"] [role="radio"]')];
  expect(choices.map((choice) => choice.getAttribute("aria-checked"))).toEqual(["true", "false", "false"]);
  await click("Dark");
  expect(setTheme).toHaveBeenCalledWith("dark");

  unmount();
  await mountSettings();
  expect(document.querySelector('[role="radiogroup"][aria-label="Theme"]')).toBeNull();
});

it("puts Export logs first in Troubleshooting and shows developer tools without unlocking", async () => {
  const onSetAcpTrace = vi.fn();
  await mountSettings({
    onResetTaskHistory: async () => undefined,
    onSetAcpTrace,
    state: {
      activeTab: "troubleshooting",
      loading: false,
      runtimeSettings: { developer: { acp_trace: { enabled: true, directory: "/var/lib/openaide/state/diagnostics/acp-traces" } } },
    },
  });

  const labels = [...document.querySelectorAll('section[aria-label="Troubleshooting"] li strong')].map((label) => label.textContent);
  expect(labels).toEqual(["Export logs", "ACP logs", "Trace folder", "Task history"]);
  expect(pageText()).toContain(".../state/diagnostics/acp-traces");

  await click("ACP logs");

  expect(onSetAcpTrace).toHaveBeenCalledWith(false);
});

it("hides developer tools until the App Server reports them", async () => {
  await mountSettings({ state: { activeTab: "troubleshooting", loading: false } });

  expect(pageText()).toContain("Export logs");
  expect(pageText()).not.toContain("ACP logs");
});

it("resets Task history only after the consequence is confirmed", async () => {
  const onResetTaskHistory = vi.fn(async () => undefined);
  await mountSettings({ onResetTaskHistory, state: { activeTab: "troubleshooting", loading: false } });

  await click("Reset task history");

  expect(onResetTaskHistory).not.toHaveBeenCalled();
  expect(dialog("Reset task history confirmation")?.textContent).toContain("This cannot be undone.");

  await click("Cancel");
  expect(dialog("Reset task history confirmation")).toBeNull();
  expect(onResetTaskHistory).not.toHaveBeenCalled();

  await click("Reset task history");
  await click("Reset task history", 1);
  await settle();

  expect(onResetTaskHistory).toHaveBeenCalledTimes(1);
  expect(dialog("Reset task history confirmation")).toBeNull();
});

it("keeps the confirmation open and says why when the reset fails", async () => {
  const onResetTaskHistory = vi.fn(async () => { throw new Error("Agents are still working."); });
  await mountSettings({ onResetTaskHistory, state: { activeTab: "troubleshooting", loading: false } });

  await click("Reset task history");
  await click("Reset task history", 1);
  await settle();

  expect(dialog("Reset task history confirmation")?.textContent).toContain("Agents are still working.");
});

it("shows a Settings failure and lets it be dismissed", async () => {
  const onDismissError = vi.fn();
  await mountSettings({ onDismissError, state: { activeTab: "general", error: "Could not save setting.", loading: false } });

  expect(document.querySelector('[role="alert"]')?.textContent).toBe("Could not save setting.");
  await click("Dismiss error");

  expect(onDismissError).toHaveBeenCalled();
});

it("opens on the section index on a phone and on the named section from a link", async () => {
  vi.stubGlobal("matchMedia", vi.fn((query: string) => ({ matches: query === "(max-width: 760px)", addEventListener: vi.fn(), removeEventListener: vi.fn() })));
  const pushed = window.location.href;

  window.history.replaceState(null, "", "/settings");
  await mountSettings();
  expect(document.querySelector(".settings-view")?.classList.contains("settings-index-open")).toBe(true);

  unmount();
  window.history.replaceState(null, "", "/settings?tab=general");
  await mountSettings();
  expect(document.querySelector(".settings-view")?.classList.contains("settings-index-open")).toBe(false);

  await click("All settings");
  expect(document.querySelector(".settings-view")?.classList.contains("settings-index-open")).toBe(true);
  window.history.replaceState(null, "", pushed);
});

it("offers phone setup checks only while the phone does its own work", async () => {
  const execute = vi.fn(async () => undefined);
  const snapshot = { remote: false, termux: true, permission: false, computer: "", computers: [] };
  vi.spyOn(frontendShell, "currentFrontendShell").mockReturnValue({
    connectionSettings: { snapshot: () => snapshot, subscribe: () => () => undefined, execute },
  } as unknown as frontendShell.FrontendShell);
  await mountSettings({ state: { activeTab: "troubleshooting", loading: false } });

  expect(pageText()).toContain("This phone");
  expect(pageText()).toContain("Termux access");
  await click("Allow");

  expect(execute).toHaveBeenCalledWith({ action: "grant" });
  expect(buttons("Share")).toHaveLength(1);
  expect(field("Search settings")).toBeTruthy();
});

function fakeDeviceIntents() {
  return {
    approveJoinRequest: vi.fn(async () => undefined),
    cancelInvite: vi.fn(async () => undefined),
    createInvite: vi.fn(async () => ({ code: "OAI1ABCDEFGH", expiresAtMs: Date.now() + 600_000 })),
    previewJoinRequest: vi.fn(),
    remove: vi.fn(async () => undefined),
  };
}
