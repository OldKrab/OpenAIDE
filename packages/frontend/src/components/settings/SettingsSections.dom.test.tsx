// @vitest-environment jsdom

import { afterEach, expect, it, vi } from "vitest";
import type { DeviceCollectionSnapshot, McpServerDefinition, WorktreeId, WorktreeRepositoryId, WorktreeSummary } from "@openaide/app-server-client";
import type { SkillSettingsDetails, SkillSettingsRecord } from "@openaide/app-shell-contracts";
import type { ProjectOption } from "../../state/composerOptions";
import * as frontendShell from "../../services/frontendShell";
import type { DesktopUpdateCapability, DesktopUpdateSnapshot, FrontendShell } from "../../services/frontendShell";
import { button, buttons, click, dialog, mountSettings, pageText, settle, type, unmount } from "./settingsTestSupport";
import type { WorktreeSettingsIntents } from "./worktreeSettingsModel";

afterEach(() => {
  unmount();
  vi.restoreAllMocks();
});

function useShell(shell: Partial<FrontendShell>) {
  vi.spyOn(frontendShell, "currentFrontendShell").mockReturnValue(shell as FrontendShell);
}

const row = (name: string) => [...document.querySelectorAll("li")].find((item) => item.querySelector("strong")?.textContent === name)!;

// MCP servers

const filesystem = {
  id: "mcp-files",
  label: "Filesystem",
  description: "Approved files",
  enabled: true,
  scope: { kind: "global" as const },
  transport: "stdio" as const,
  status: "configured" as const,
};
const filesystemDefinition = {
  id: "mcp-files",
  label: "Filesystem",
  description: "Approved files",
  enabled: true,
  scope: { kind: "global" },
  configuration: { transport: "stdio", commandLine: "/usr/bin/mcp serve", command: "/usr/bin/mcp", args: ["serve"], secretEnv: ["TOKEN"] },
} as McpServerDefinition;
const mcpState = (servers = [filesystem]) => ({
  activeTab: "mcp" as const,
  availableTabs: ["general" as const, "mcp" as const],
  loading: false,
  mcpServers: servers,
  mcpServersAvailability: "available" as const,
});

it("turns an MCP server off from the list", async () => {
  const onSetMcpServerEnabled = vi.fn();
  await mountSettings({ onSetMcpServerEnabled, state: mcpState() });

  await click("Filesystem enabled");

  expect(onSetMcpServerEnabled).toHaveBeenCalledWith("mcp-files", false);
});

it("opens an MCP server with its configuration and keeps stored secrets write-only", async () => {
  const onGetMcpServerDetails = vi.fn(async () => filesystemDefinition);
  await mountSettings({ onGetMcpServerDetails, state: mcpState() });

  await click("Open Filesystem");
  await settle();

  expect(onGetMcpServerDetails).toHaveBeenCalledWith("mcp-files");
  const page = document.querySelector('section[aria-label="MCP server details"]')!;
  expect(page.querySelector<HTMLInputElement>('input[aria-label="Command"]')?.value).toBe("/usr/bin/mcp serve");
  expect(page.querySelector<HTMLInputElement>('input[aria-label="Environment name"]')?.value).toBe("TOKEN");
  expect(page.querySelector<HTMLInputElement>('input[aria-label="Environment value"]')?.value).toBe("");
  // Nothing changed yet, so there is nothing to save.
  expect(buttons("Save")).toHaveLength(0);
  expect(buttons("Delete server…")).toHaveLength(1);
});

it("adds an MCP server and refuses a new secret that has no value", async () => {
  const onSaveMcpServer = vi.fn();
  await mountSettings({ onSaveMcpServer, state: mcpState([]) });

  await click("Add server");
  await type("Server name", "Filesystem");
  await type("Command", "/usr/bin/mcp serve");
  await click("Add variable");
  await type("Environment name", "TOKEN");
  await click("TOKEN is secret");
  await click("Add server");

  expect(onSaveMcpServer).not.toHaveBeenCalled();
  expect(document.querySelector('[role="alert"]')?.textContent).toContain("Enter a value for the new secret TOKEN");

  await type("Environment value", "s3cret");
  await click("Add server");

  expect(onSaveMcpServer).toHaveBeenCalledTimes(1);
  expect(onSaveMcpServer.mock.calls[0][0]).toMatchObject({
    server: { label: "Filesystem", configuration: { transport: "stdio", commandLine: "/usr/bin/mcp serve", secretEnv: ["TOKEN"] } },
    previous: undefined,
    secretValues: [{ field: "env", name: "TOKEN", value: "s3cret" }],
  });
});

it("deletes an MCP server only after confirmation", async () => {
  const onDeleteMcpServer = vi.fn();
  await mountSettings({ onDeleteMcpServer, onGetMcpServerDetails: async () => filesystemDefinition, state: mcpState() });

  await click("Open Filesystem");
  await settle();
  await click("Delete server…");

  expect(onDeleteMcpServer).not.toHaveBeenCalled();
  await click("Delete server");
  await settle();

  expect(onDeleteMcpServer).toHaveBeenCalledWith(filesystemDefinition);
});

it("says when MCP configuration cannot be read", async () => {
  await mountSettings({ state: { ...mcpState([]), mcpServersAvailability: "unavailable" } });

  expect(pageText()).toContain("MCP settings unavailable");
  expect(buttons("Add server")).toHaveLength(0);
});

// Skills

const research: SkillSettingsRecord = {
  id: "skill-opaque",
  label: "Research",
  scope: "global",
  source_label: "User configuration",
  status: "valid",
  description: "Find primary sources",
  warnings: [],
  tags: [],
  last_scanned_at: "now",
};

it("opens a Skill with its description, file, and instructions, and folds long metadata", async () => {
  const details: SkillSettingsDetails = {
    generated_at: "now",
    skill: research,
    document: {
      name: "Research",
      description: "Find primary sources",
      additional_fields: ["license", "compatibility", "model", "context", "agent", "version", "author"].map((name) => ({ name, value: `${name} value` })),
      instructions: "# Workflow\n\nRead every source.",
      source: "---\nname: Research\n---\n# Workflow",
    },
  };
  const onGetSkillDetails = vi.fn(async () => details);
  await mountSettings({
    onGetSkillDetails,
    state: { activeTab: "skills", availableTabs: ["general", "skills"], loading: false, skills: [research], skillsAvailability: "available" },
  });

  await click("Open Research");
  await settle();

  expect(onGetSkillDetails).toHaveBeenCalledWith("skill-opaque");
  const page = document.querySelector('article[aria-label="Skill details"]')!;
  const headings = () => [...page.querySelectorAll("h2")].map((heading) => heading.textContent);
  // The file sits above the instructions so it is not below a long document.
  expect(headings()).toEqual(["Settings", "File", "Instructions"]);
  expect(page.textContent).toContain("Find primary sources");
  expect(page.textContent).toContain("SKILL.md");
  expect(page.textContent).toContain("Read every source.");
  expect(page.querySelectorAll("dt")).toHaveLength(5);

  await click("Show all 7");
  expect(page.querySelectorAll("dt")).toHaveLength(7);

  await click("Source");
  expect(page.querySelector("pre")?.textContent).toContain("name: Research");
});

// Worktrees

const project: ProjectOption = {
  label: "OpenAIDE",
  projectId: "project-1",
  projectWorktreeId: "project-root",
  workspaceRoot: "/work/OpenAIDE",
  worktreeRepositoryId: "repository-1",
} as ProjectOption;

function worktree(overrides: Partial<WorktreeSummary> = {}): WorktreeSummary {
  return {
    availability: "available",
    forgotten: false,
    head: { kind: "branch", name: "feature/search", commit: "53cbe91d" },
    isMain: false,
    linkedTaskCount: 1,
    name: "Search",
    ownership: "managed",
    path: "/work/OpenAIDE/.worktrees/search",
    runningTaskCount: 0,
    worktreeId: "search" as WorktreeId,
    ...overrides,
  } as WorktreeSummary;
}

function worktreeIntents(overrides: Partial<WorktreeSettingsIntents> = {}): WorktreeSettingsIntents {
  return {
    createWorktree: vi.fn(),
    openTask: vi.fn(),
    recreateWorktree: vi.fn(),
    refreshWorktrees: vi.fn(async () => undefined),
    removalPreflight: vi.fn(),
    removeWorktree: vi.fn(async () => undefined),
    ...overrides,
  } as WorktreeSettingsIntents;
}

const worktreeProps = (intents: WorktreeSettingsIntents, onNewTaskInWorktree = vi.fn()) => ({
  onNewTaskInWorktree,
  projects: [project],
  state: { activeTab: "worktrees" as const, loading: false },
  worktreeIntents: intents,
  worktreeRepositories: { "repository-1": { repositoryId: "repository-1" as WorktreeRepositoryId, revision: 1, worktrees: [worktree()] } },
});

it("reads worktrees again when the section is shown and starts a Task in one", async () => {
  const intents = worktreeIntents();
  const onNewTask = vi.fn();
  await mountSettings(worktreeProps(intents, onNewTask));
  await settle();

  expect(intents.refreshWorktrees).toHaveBeenCalledWith(project);
  expect(pageText()).toContain("OpenAIDE");

  await click("Open Search");
  expect(document.querySelector('article[aria-label="Worktree details"]')?.textContent).toContain("/work/OpenAIDE/.worktrees/search");
  await click("New task here");

  expect(onNewTask).toHaveBeenCalledWith(project, expect.objectContaining({ worktreeId: "search" }));
});

it("removes a worktree only after the App Server says it is safe and the user confirms", async () => {
  const intents = worktreeIntents({ removalPreflight: vi.fn(async () => ({ status: "safe" })) as never });
  await mountSettings(worktreeProps(intents));

  await click("Open Search");
  await click("Remove worktree…");
  await settle();

  expect(intents.removeWorktree).not.toHaveBeenCalled();
  expect(dialog("Remove worktree confirmation")?.textContent).toContain("The folder will be deleted.");
  await click("Remove");
  await settle();

  expect(intents.removeWorktree).toHaveBeenCalledTimes(1);
});

it("offers Worktrees only where the client can manage them", async () => {
  await mountSettings({ state: { activeTab: "general", loading: false } });

  expect(buttons("Worktrees")).toHaveLength(0);
});

// Remote

const phone = { deviceId: "device_1", name: "Phone", model: "Example 9", addedAtMs: 1_700_000_000_000, addedBy: "workstation" };

function deviceIntents() {
  return {
    approveJoinRequest: vi.fn(async () => undefined),
    cancelInvite: vi.fn(async () => undefined),
    createInvite: vi.fn(async () => ({ code: "OAI1ABCDEFGH", expiresAtMs: Date.now() + 600_000 })),
    previewJoinRequest: vi.fn(async () => ({ deviceId: "device_1", name: "Phone", model: "Example 9", serverName: "workstation", alreadyTrusted: false })),
    remove: vi.fn(async () => undefined),
    replacementImpact: vi.fn(async () => ({ activeTurns: 0, pendingTaskRequests: 0 })),
  };
}

const remoteState = (devices: DeviceCollectionSnapshot) => ({ activeTab: "remote" as const, devices, loading: false });

it("names the computer whose devices are listed and removes one after confirmation", async () => {
  const intents = deviceIntents();
  await mountSettings({ deviceIntents: intents, state: remoteState({ remoteAccess: "on", serverName: "workstation", devices: [phone] }) });

  expect([...document.querySelectorAll("h2")].map((heading) => heading.textContent)).toContain("Devices paired with workstation");
  expect(row("Phone").textContent).toContain("Example 9");

  await click("Remove Phone");
  expect(intents.remove).not.toHaveBeenCalled();
  await click("Remove device");
  await settle();

  expect(intents.remove).toHaveBeenCalledWith("device_1");
});

it("shows a single-use pairing code, reports the device that used it, and withdraws the code on close", async () => {
  const intents = deviceIntents();
  const snapshot: DeviceCollectionSnapshot = { remoteAccess: "on", serverName: "workstation" };
  await mountSettings({ deviceIntents: intents, state: remoteState(snapshot) });

  await click("Show code");
  await settle();

  expect(intents.createInvite).toHaveBeenCalledOnce();
  expect(document.querySelector('[aria-label="Pairing code as a QR code"]')).not.toBeNull();
  expect(document.querySelector('[aria-label="Pairing code text"]')?.textContent).toBe("OAI1 ABCD EFGH");

  await mountSettings({ deviceIntents: intents, state: remoteState({ ...snapshot, devices: [phone] }) });
  expect(dialog("Pairing code")?.textContent).toContain("Phone can now use this OpenAIDE.");

  await click("Done");
  expect(intents.cancelInvite).toHaveBeenCalled();
});

it("adds a device from its code only after showing what the code names", async () => {
  const intents = deviceIntents();
  await mountSettings({ deviceIntents: intents, state: remoteState({ remoteAccess: "on", serverName: "workstation" }) });

  await click("Enter code");
  expect((button("Continue") as HTMLButtonElement).disabled).toBe(true);
  await type("Code from the device", "OAJ1 CODE");
  await click("Continue");
  await settle();

  expect(intents.approveJoinRequest).not.toHaveBeenCalled();
  expect(dialog("Add a device by code")?.textContent).toContain("full access");
  await click("Add device");
  await settle();

  expect(intents.approveJoinRequest).toHaveBeenCalledWith("OAJ1 CODE");
  expect(dialog("Add a device by code")).toBeNull();
});

const studio = { id: "a".repeat(64), name: "Studio", current: false };

function remoteComputers(computers = [studio]) {
  return {
    list: vi.fn().mockResolvedValue(computers),
    pair: vi.fn().mockResolvedValue(undefined),
    use: vi.fn().mockResolvedValue(undefined),
    forget: vi.fn().mockResolvedValue(undefined),
  };
}

it("shows which computer the Desktop is connected to and switches to another", async () => {
  const computers = remoteComputers();
  useShell({ remoteComputers: computers });
  await mountSettings({ state: { activeTab: "remote", loading: false } });
  await settle();

  expect(row("This computer").textContent).toContain("In use");
  await click("Use Studio");
  await settle();

  expect(computers.use).toHaveBeenCalledWith(studio.id);
});

it("forgets a paired computer only after saying what happens", async () => {
  const computers = remoteComputers([{ ...studio, current: true }]);
  useShell({ remoteComputers: computers });
  await mountSettings({ state: { activeTab: "remote", loading: false } });
  await settle();

  await click("Forget Studio");
  expect(computers.forget).not.toHaveBeenCalled();
  expect(document.querySelector('[role="dialog"]')?.textContent).toContain("works on this computer");
  await click("Forget");
  await settle();

  expect(computers.forget).toHaveBeenCalledWith(studio.id);
});

it("pairs with a pasted code and shows the shell's reason when it is refused", async () => {
  const computers = remoteComputers([]);
  computers.pair.mockRejectedValue("This code has expired or was already used.");
  useShell({ remoteComputers: computers });
  await mountSettings({ state: { activeTab: "remote", loading: false } });
  await settle();

  await click("Pair a computer");
  expect((button("Pair") as HTMLButtonElement).disabled).toBe(true);
  await type("Pairing code", "OAI1 ABCD");
  await click("Pair");
  await settle();

  expect(computers.pair).toHaveBeenCalledWith("OAI1 ABCD");
  expect(dialog("Pair with another computer")?.textContent).toContain("already used");
});

const devbox = { id: "b".repeat(64), name: "devbox", current: true, ssh: "me@devbox" };

function sshComputers(status = { computerId: devbox.id, runningVersion: "0.9.0", updateAvailable: false, suggestLinger: false }) {
  return {
    hosts: vi.fn().mockResolvedValue(["devbox", "build-1"]),
    add: vi.fn().mockResolvedValue(undefined),
    status: vi.fn().mockResolvedValue(status),
    update: vi.fn().mockResolvedValue(undefined),
    stop: vi.fn().mockResolvedValue("stopped"),
  };
}

it("adds an SSH computer by host and offers the hosts from the SSH configuration", async () => {
  const ssh = sshComputers();
  useShell({ remoteComputers: { ...remoteComputers([]), ssh } });
  await mountSettings({ state: { activeTab: "remote", loading: false } });
  await settle();

  await click("Add SSH computer");
  await settle();
  expect([...document.querySelectorAll("#settings-ssh-hosts option")].map((option) => option.getAttribute("value"))).toEqual(["devbox", "build-1"]);
  expect((button("Add and connect") as HTMLButtonElement).disabled).toBe(true);
  await type("Host", " devbox ");
  await click("Add and connect");
  await settle();

  expect(ssh.add).toHaveBeenCalledWith("devbox", undefined);
});

it("names an SSH computer by its destination and stops OpenAIDE there only after confirmation", async () => {
  const ssh = sshComputers();
  useShell({ remoteComputers: { ...remoteComputers([devbox]), current: "devbox", ssh } });
  await mountSettings({ state: { activeTab: "remote", loading: false } });
  await settle();

  expect(row("devbox").textContent).toContain("SSH · me@devbox");
  await click("Stop OpenAIDE on devbox");
  expect(ssh.stop).not.toHaveBeenCalled();
  expect(dialog("Stop OpenAIDE on devbox?")?.textContent).toContain("ending work in progress");
  await click("Stop");
  await settle();

  expect(ssh.stop).toHaveBeenCalledWith(devbox.id);
  expect(pageText()).toContain("OpenAIDE on devbox stops once no app is connected to it.");
});

it("offers to update an older App Server on the SSH computer and lists what the update interrupts", async () => {
  const ssh = sshComputers({ computerId: devbox.id, runningVersion: "0.9.0", updateAvailable: true, suggestLinger: true });
  useShell({ remoteComputers: { ...remoteComputers([devbox]), current: "devbox", ssh } });
  const intents = {
    ...deviceIntents(),
    replacementImpact: vi.fn().mockResolvedValue({
      otherClients: [{ kind: "web", appVersion: "0.9.0", reconnecting: false }, { kind: "keeper", reconnecting: false }],
      activeTurns: 2,
      pendingTaskRequests: 1,
    }),
  };
  await mountSettings({ deviceIntents: intents, state: { activeTab: "remote", loading: false } });
  await settle();

  expect(pageText()).toContain("devbox runs OpenAIDE 0.9.0, older than this app.");
  expect(pageText()).toContain("loginctl enable-linger");
  await click("Update…");
  await settle();
  const update = dialog("Update OpenAIDE on devbox?")!;
  expect([...update.querySelectorAll("li")].map((item) => item.textContent)).toEqual([
    "OpenAIDE in a browser 0.9.0",
    "Agents working in 2 Tasks",
    "1 question waiting for an answer",
  ]);
  expect(ssh.update).not.toHaveBeenCalled();
  await click("Update and restart");
  await settle();

  expect(ssh.update).toHaveBeenCalled();
});

function phoneShell(snapshot: object) {
  const execute = vi.fn(async () => undefined);
  useShell({ connectionSettings: { snapshot: () => snapshot, subscribe: () => () => undefined, execute } } as unknown as Partial<FrontendShell>);
  return execute;
}

const phoneDevbox = { id: "c".repeat(32), name: "devbox", destination: "me@devbox:2222", current: true };
const phoneLab = { id: "d".repeat(32), name: "lab", destination: "me@lab", current: false };

it("lists the phone's SSH computers beside paired ones and adds one through the phone", async () => {
  const execute = phoneShell({ remote: true, computer: "devbox", computers: [], sshComputers: [phoneDevbox, phoneLab], sshAvailable: true, ssh: null });
  await mountSettings({ state: { activeTab: "remote", loading: false } });
  await settle();

  expect(row("devbox").textContent).toContain("SSH · me@devbox:2222");
  await click("Use lab");
  expect(execute).toHaveBeenCalledWith({ action: "use_ssh", id: phoneLab.id });
  await click("Add SSH computer");
  expect(execute).toHaveBeenCalledWith({ action: "ssh_add" });
});

it("offers no SSH computers on a phone build that cannot install on them", async () => {
  phoneShell({ remote: false, computer: "", computers: [], sshComputers: [], sshAvailable: false });
  await mountSettings({ state: { activeTab: "remote", loading: false } });
  await settle();

  expect(buttons("Add SSH computer")).toHaveLength(0);
});

it("stops and updates the phone's SSH computer only after their warnings", async () => {
  const execute = phoneShell({
    remote: true, computer: "devbox", computers: [], sshComputers: [phoneDevbox], sshAvailable: true,
    ssh: { runningVersion: "0.9.0", updateAvailable: true, suggestLinger: false }, sshNotice: "",
  });
  const intents = { ...deviceIntents(), replacementImpact: vi.fn().mockResolvedValue({ otherClients: [], activeTurns: 1, pendingTaskRequests: 0 }) };
  await mountSettings({ deviceIntents: intents, state: { activeTab: "remote", loading: false } });
  await settle();

  await click("Stop OpenAIDE on devbox");
  expect(execute).not.toHaveBeenCalledWith({ action: "stop_ssh", id: phoneDevbox.id });
  await click("Stop");
  await settle();
  expect(execute).toHaveBeenCalledWith({ action: "stop_ssh", id: phoneDevbox.id });

  expect(pageText()).toContain("devbox runs OpenAIDE 0.9.0, older than this app.");
  await click("Update…");
  await settle();
  expect(dialog("Update OpenAIDE on devbox?")?.textContent).toContain("Agents working in 1 Task");
  await click("Update and restart");
  await settle();
  expect(execute).toHaveBeenCalledWith({ action: "update_ssh" });
});

// General: Desktop runtime and updates

it("switches where Agents run and leaves the environment in use alone", async () => {
  const select = vi.fn(async () => undefined);
  useShell({ desktopRuntime: { snapshot: () => ({ active: { kind: "native" }, wslDistros: ["Ubuntu"] }), select } });
  await mountSettings();

  expect(row("Windows").textContent).toContain("In use");
  expect(buttons("Use Windows")).toHaveLength(0);
  await click("Use WSL");

  expect(select).toHaveBeenCalledWith({ kind: "wsl", distro: "Ubuntu" });
});

it("hides the runtime choice while a paired computer does the work", async () => {
  useShell({
    desktopRuntime: { snapshot: () => ({ active: { kind: "native" }, wslDistros: ["Ubuntu"] }), select: vi.fn() },
    remoteComputers: { ...remoteComputers(), current: "Studio" },
  });
  await mountSettings();

  expect(pageText()).not.toContain("Agents run in");
});

function updates(initial: DesktopUpdateSnapshot) {
  let snapshot = initial;
  const listeners = new Set<() => void>();
  const capability: DesktopUpdateCapability = {
    snapshot: () => snapshot,
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    check: vi.fn(async () => undefined),
    download: vi.fn(async () => undefined),
    cancelDownload: vi.fn(async () => undefined),
    restartAndUpdate: vi.fn(async () => "started" as const),
    openReleaseNotes: vi.fn(async () => undefined),
  };
  return capability;
}

it("downloads an available update and shows its release notes", async () => {
  const capability = updates({ revision: 2, installedVersion: "1.0.0", kind: "available", offer: { version: "1.1.0", notes: "## Faster startup", sizeBytes: 42 * 1024 * 1024 } });
  useShell({ desktopUpdates: capability });
  await mountSettings();

  expect(row("OpenAIDE 1.0.0").textContent).toContain("Version 1.1.0 is available");
  expect(row("OpenAIDE 1.0.0").textContent).toContain("Faster startup");
  await click("Download update");

  expect(capability.download).toHaveBeenCalledOnce();
});

it("keeps a ready update one click away after Not now", async () => {
  const capability = updates({ revision: 3, installedVersion: "1.0.0", kind: "readyToUpdate", offer: { version: "1.1.0", notes: "Fixes", sizeBytes: 10 } });
  useShell({ desktopUpdates: capability });
  await mountSettings();

  await click("Not now");

  expect(buttons("Show update")).toHaveLength(1);
  expect(capability.restartAndUpdate).not.toHaveBeenCalled();
});

it("asks before an update stops active work", async () => {
  const capability = updates({ revision: 3, installedVersion: "1.0.0", kind: "readyToUpdate", offer: { version: "1.1.0", notes: "Fixes", sizeBytes: 10 } });
  vi.mocked(capability.restartAndUpdate).mockResolvedValueOnce("activeWork").mockResolvedValueOnce("started");
  useShell({ desktopUpdates: capability });
  await mountSettings();

  await click("Restart and update");
  await settle();
  expect(pageText()).toContain("OpenAIDE is still working.");

  await click("Stop work and update");
  await settle();

  expect(capability.restartAndUpdate).toHaveBeenNthCalledWith(2, { stopActiveWork: true });
});

it("hands a ready update to the system installer instead of restarting", async () => {
  const capability = { ...updates({ revision: 3, installedVersion: "1.0.0", kind: "readyToUpdate", offer: { version: "1.1.0", notes: "Fixes", sizeBytes: 10 } }), installation: "systemInstaller" as const };
  useShell({ desktopUpdates: capability });
  await mountSettings();

  expect(row("OpenAIDE 1.0.0").textContent).toContain("OpenAIDE closes while it installs.");
  expect(buttons("Restart and update")).toHaveLength(0);
  await click("Install update");

  expect(capability.restartAndUpdate).toHaveBeenCalledOnce();
});

it("explains why a development build cannot update", async () => {
  useShell({ desktopUpdates: updates({ revision: 1, installedVersion: "1.0.0", kind: "unavailable", unavailableReason: "developmentBuild" }) });
  await mountSettings();

  expect(row("OpenAIDE 1.0.0").textContent).toContain("development builds");
});
