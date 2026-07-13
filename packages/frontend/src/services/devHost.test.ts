import { afterEach, describe, expect, it, vi } from "vitest";
import type { HostToWebviewMessage, WebviewToHostMessage } from "@openaide/app-shell-contracts";
import { createStandaloneHost } from "../../../../apps/browser/frontend/standaloneHost";
import { standaloneBootstrapFrom } from "../../../../apps/browser/frontend/standaloneHostBootstrap";
import { handleStandaloneHostMessage } from "../../../../apps/browser/frontend/standaloneHostRouter";

describe("standalone dev host", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });

  it("disables standalone bootstrap inside VS Code or prebootstrapped webviews", () => {
    expect(standaloneBootstrapFrom({ hasDatasetSurface: false, hasVsCodeApi: true, pathname: "/task" })).toBeUndefined();
    expect(standaloneBootstrapFrom({ hasDatasetSurface: true, hasVsCodeApi: false, pathname: "/task" })).toBeUndefined();
  });

  it("maps standalone paths to preview surfaces", () => {
    expect(standaloneBootstrapFrom({ hasDatasetSurface: false, hasVsCodeApi: false, pathname: "/task" })).toMatchObject({
      surface: "task",
      taskId: "demo_task",
      preferences: { composer_submit_shortcut: "enter" },
    });
    expect(standaloneBootstrapFrom({ hasDatasetSurface: false, hasVsCodeApi: false, pathname: "/new-task" })).toMatchObject({
      surface: "task",
      taskId: undefined,
    });
    expect(standaloneBootstrapFrom({ hasDatasetSurface: false, hasVsCodeApi: false, pathname: "/navigation" })).toMatchObject({
      surface: "navigation",
      taskId: undefined,
    });
    expect(standaloneBootstrapFrom({ hasDatasetSurface: false, hasVsCodeApi: false, pathname: "/settings" })).toMatchObject({
      surface: "settings",
      taskId: undefined,
    });
  });

  it("ignores invalid messages", () => {
    const output = outputSpy();
    handleStandaloneHostMessage(null, output);
    handleStandaloneHostMessage({ payload: {} }, output);
    handleStandaloneHostMessage({ type: 42 }, output);

    expect(output.post).not.toHaveBeenCalled();
    expect(output.navigate).not.toHaveBeenCalled();
  });

  it("routes shell-local responses", () => {
    const output = outputSpy();
    handleStandaloneHostMessage({ type: "workspace.roots" }, output);
    handleStandaloneHostMessage({ type: "developer.settings.unlock" }, output);

    expect(posted(output, "workspace.roots.result").payload.roots.length).toBeGreaterThan(0);
    expect(posted(output, "runtime.settings.result").payload).toMatchObject({ developer: { acp_trace: { enabled: true } } });
  });

  it("ignores legacy product requests", () => {
    const output = outputSpy();

    for (const message of [
      { type: "task.list", payload: { archived: true } },
      { type: "agent.listSessions", payload: { agent_id: "codex" } },
      { type: "agent.configOptions", payload: { agent_id: "codex", workspace_root: "demo-project" } },
      { type: "task.snapshot", payload: { task_id: "demo_task" } },
      { type: "settings.snapshot" },
      { type: "task.create", payload: { title: "New task" } },
      { type: "session.prompt", payload: { task_id: "demo_task", text: "Prompt" } },
    ]) {
      handleStandaloneHostMessage(message, output);
    }

    expect(output.post).not.toHaveBeenCalled();
    expect(output.navigate).not.toHaveBeenCalled();
  });

  it("routes standalone surface navigation through injected navigation output", () => {
    const output = outputSpy();
    handleStandaloneHostMessage({ type: "surface.openTask", payload: { task_id: "demo_task" } }, output);
    handleStandaloneHostMessage({ type: "surface.openNewTask" }, output);
    handleStandaloneHostMessage({ type: "surface.openSettings" }, output);

    expect(output.navigate).toHaveBeenNthCalledWith(1, "/task");
    expect(output.navigate).toHaveBeenNthCalledWith(2, "/new-task");
    expect(output.navigate).toHaveBeenNthCalledWith(3, "/settings");
    expect(output.post).not.toHaveBeenCalled();
  });

  it("wires standalone facade navigation through pushState and reload", () => {
    const pushState = vi.fn();
    const reload = vi.fn();
    stubBrowser({ history: { pushState }, location: { reload } });

    createStandaloneHost()?.postMessage({ type: "surface.openSettings" });

    expect(pushState).toHaveBeenCalledWith(null, "", "/settings");
    expect(reload).toHaveBeenCalledOnce();
  });

  it("wires standalone facade posts through async browser message dispatch", () => {
    vi.useFakeTimers();
    const dispatched: unknown[] = [];
    stubBrowser({
      dispatchEvent: vi.fn((event: Event) => {
        dispatched.push((event as MessageEvent).data);
        return true;
      }),
      setTimeout: (callback: () => void) => {
        globalThis.setTimeout(callback, 0);
        return 1;
      },
    });

    createStandaloneHost()?.postMessage({ type: "developer.settings.unlock" });

    expect(dispatched).toEqual([]);
    vi.runAllTimers();
    expect(dispatched).toEqual([expect.objectContaining({ type: "runtime.settings.result" })]);
  });
});

function outputSpy() {
  return {
    post: vi.fn<(message: HostToWebviewMessage) => void>(),
    navigate: vi.fn<(path: string) => void>(),
  };
}

function posted<T extends HostToWebviewMessage["type"]>(output: ReturnType<typeof outputSpy>, type: T) {
  const match = output.post.mock.calls.map(([message]) => message).find((message) => message.type === type);
  if (!match) throw new Error(`Expected ${type} response.`);
  return match as Extract<HostToWebviewMessage, { type: T }>;
}

type BrowserStub = {
  acquireVsCodeApi?: Window["acquireVsCodeApi"];
  dispatchEvent?: ReturnType<typeof vi.fn>;
  history?: { pushState?: ReturnType<typeof vi.fn> };
  location?: { pathname?: string; reload?: ReturnType<typeof vi.fn> };
  setTimeout?: (callback: () => void) => number;
};

function stubBrowser(overrides: BrowserStub) {
  vi.stubGlobal("document", {
    body: {
      dataset: {},
    },
  });
  vi.stubGlobal("window", {
    acquireVsCodeApi: undefined,
    dispatchEvent: vi.fn(),
    history: { pushState: vi.fn(), ...overrides.history },
    location: { pathname: "/task", reload: vi.fn(), ...overrides.location },
    setTimeout: (callback: () => void) => {
      callback();
      return 1;
    },
    ...overrides,
  });
}
