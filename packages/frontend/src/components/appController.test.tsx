import { act, create } from "react-test-renderer";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  AGENT_LIST_SESSIONS,
  ATTACHMENT_CREATE_PASTED_IMAGE,
  AppServerProtocolError,
  SETTINGS_GET_AGENT_DETAILS,
  SETTINGS_GET_MCP_SERVERS,
  SETTINGS_GET_SKILLS,
  STATE_SUBSCRIBE,
  TASK_CREATE,
  TASK_DISCARD,
  TASK_LIST,
  TASK_OPEN,
  TASK_SEND,
  TASK_SET_CONFIG_OPTION,
  type BackendConnection,
  type ClientSnapshot,
  type InitializeParams,
  type InitializeResult,
} from "@openaide/app-server-client";
import type { HostToWebviewMessage, TaskSnapshot } from "@openaide/app-shell-contracts";
import { projectIdForWorkspaceRoot } from "../state/projectIdentity";
import { type AppController, useAppController } from "./appController";

const postHostMessage = vi.fn();
const updateWebSettingsTabRoute = vi.fn();
const listeners: Array<(message: HostToWebviewMessage) => void> = [];
const webRouteListeners: Array<(nextBootstrap: TestBootstrap) => void> = [];
let bootstrap: TestBootstrap = navigationBootstrap();
let backendConnection: TestBackendConnection | undefined;
let latestController: AppController | undefined;

vi.mock("../services/hostBridge", () => ({
  getBackendConnection: () => backendConnection,
  getBootstrap: () => bootstrap,
  postHostMessage: (message: unknown) => postHostMessage(message),
  updateWebSettingsTabRoute: (tab: unknown) => updateWebSettingsTabRoute(tab),
  subscribeWebRouteChanges: (listener: (nextBootstrap: TestBootstrap) => void) => {
    webRouteListeners.push(listener);
    return () => {
      const index = webRouteListeners.indexOf(listener);
      if (index >= 0) webRouteListeners.splice(index, 1);
    };
  },
  subscribeHostMessages: (listener: (message: HostToWebviewMessage) => void) => {
    listeners.push(listener);
    return () => {
      const index = listeners.indexOf(listener);
      if (index >= 0) listeners.splice(index, 1);
    };
  },
}));

function ControllerProbe() {
  latestController = useAppController();
  return null;
}

function legacyHostMessage(message: unknown): HostToWebviewMessage {
  return message as HostToWebviewMessage;
}

describe("app controller mounted lifecycle", () => {
  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    vi.useFakeTimers();
    vi.stubGlobal("window", {
      clearTimeout: globalThis.clearTimeout,
      setTimeout: globalThis.setTimeout,
    });
    vi.stubGlobal("sessionStorage", memoryStorage());
    postHostMessage.mockClear();
    updateWebSettingsTabRoute.mockClear();
    listeners.length = 0;
    webRouteListeners.length = 0;
    bootstrap = navigationBootstrap();
    backendConnection = undefined;
    latestController = undefined;
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("starts navigation with telemetry, workspace roots, and local task-list error", () => {
    act(() => {
      create(<ControllerProbe />);
    });

    expect(postHostMessage).toHaveBeenCalledWith({
      type: "webview.telemetry",
      payload: expect.objectContaining({ event: "started", surface: "navigation" }),
    });
    expect(postHostMessage).toHaveBeenCalledWith({ type: "workspace.roots" });
    expect(latestController?.state.taskListError).toBe("App Server connection unavailable.");
    expect(latestController?.state.appServerError).toBe("App Server connection unavailable.");
    expect(postHostMessage).not.toHaveBeenCalledWith(expect.objectContaining({ type: "task.list" }));
  });

  it("initializes the App Server connection for the current surface", async () => {
    const initialize = vi.fn(async (_params: InitializeParams) => ({ snapshot: clientSnapshot() }));
    const close = vi.fn();
    backendConnection = { initialize, request: vi.fn(), respond: vi.fn(), close };
    bootstrap = taskBootstrap("task_1");

    let mounted: ReturnType<typeof create>;
    await act(async () => {
      mounted = create(<ControllerProbe />);
      await Promise.resolve();
    });

    expect(initialize).toHaveBeenCalledWith(
      expect.objectContaining({
        shell: { kind: "vscodeExtension" },
        requestedSurface: { kind: "task", taskId: "task_1" },
      }),
    );

    act(() => {
      mounted.unmount();
    });
    expect(close).toHaveBeenCalledOnce();
  });

  it("uses App Server Agent snapshots for new-task Agent selection", async () => {
    backendConnection = {
      initialize: vi.fn(async () => ({
        snapshot: clientSnapshot({
          agents: [
            { agentId: "opencode" as never, label: "OpenCode", status: "disconnected" },
            { agentId: "custom.one" as never, label: "Custom One", status: "disconnected" },
          ],
          defaultAgentId: "custom.one",
        }),
      })),
      request: vi.fn(),
      respond: vi.fn(),
      close: vi.fn(),
    };

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
    });

    expect(latestController?.agents?.map((agent) => agent.id)).toEqual(["opencode", "custom.one"]);
    expect(latestController?.state.newTask.selection).toMatchObject({
      agentId: "custom.one",
      agentLabel: "Custom One",
    });
  });

  it("uses App Server app preferences from initialize snapshots", async () => {
    backendConnection = {
      initialize: vi.fn(async () => ({
        snapshot: clientSnapshot({
          appPreferences: { preferences: { composerSubmitShortcut: "enter" } },
        }),
      })),
      request: vi.fn(),
      respond: vi.fn(),
      close: vi.fn(),
    };

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
    });

    expect(latestController?.preferences).toEqual({ composer_submit_shortcut: "enter" });
  });

  it("loads settings details through App Server on connected settings startup", async () => {
    const request = vi.fn(async (method: string) => {
      if (method === SETTINGS_GET_MCP_SERVERS) {
        return { generatedAt: "mcp-now", availability: "unavailable", servers: [] };
      }
      if (method === SETTINGS_GET_SKILLS) {
        return { generatedAt: "skills-now", availability: "unavailable", skills: [] };
      }
      return {
        generatedAt: "now",
        agents: [{
          agentId: "codex",
          label: "Codex",
          enabled: true,
          sourceKind: "builtIn",
          icon: "bot",
          transport: "stdio",
          status: "connected",
          launchLabel: "Codex",
          env: [],
          description: "Codex Agent",
          capabilities: [],
          authMethods: [],
        }],
      };
    });
    backendConnection = {
      initialize: vi.fn(async () => ({
        snapshot: clientSnapshot({
          runtimeSettings: {
            developer: {
              acpTrace: { enabled: true, directory: "/runtime/traces" },
            },
          },
          settingsSections: ["agents", "mcpServers", "skills", "commonSettings"],
        }),
      })),
      request: request as unknown as BackendConnection["request"],
      respond: vi.fn(),
      close: vi.fn(),
    };
    bootstrap = settingsBootstrap();

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(request).toHaveBeenCalledWith(SETTINGS_GET_AGENT_DETAILS, {});
    expect(request).toHaveBeenCalledWith(SETTINGS_GET_MCP_SERVERS, {});
    expect(request).toHaveBeenCalledWith(SETTINGS_GET_SKILLS, {});
    expect(latestController?.state.settings.agentDetails?.[0]).toMatchObject({
      id: "codex",
      status: "connected",
    });
    expect(latestController?.state.settings.runtimeSettings?.developer.acp_trace).toEqual({
      enabled: true,
      directory: "/runtime/traces",
    });
    expect(latestController?.state.settings.availableTabs).toEqual(["agents", "mcp", "skills", "common"]);
    expect(latestController?.state.settings.mcpServers).toEqual([]);
    expect(latestController?.state.settings.mcpServersAvailability).toBe("unavailable");
    expect(latestController?.state.settings.skills).toEqual([]);
    expect(latestController?.state.settings.skillsAvailability).toBe("unavailable");
    expect(postHostMessage).not.toHaveBeenCalledWith({ type: "settings.snapshot" });
  });

  it("selects the settings tab requested by the web route", async () => {
    const request = vi.fn(async (method: string) => {
      if (method === SETTINGS_GET_MCP_SERVERS) {
        return { generatedAt: "mcp-now", availability: "unavailable", servers: [] };
      }
      if (method === SETTINGS_GET_SKILLS) {
        return { generatedAt: "skills-now", availability: "unavailable", skills: [] };
      }
      return { generatedAt: "now", agents: [] };
    });
    backendConnection = {
      initialize: vi.fn(async () => ({
        snapshot: clientSnapshot({
          settingsSections: ["agents", "mcpServers", "skills", "commonSettings"],
        }),
      })),
      request: request as unknown as BackendConnection["request"],
      respond: vi.fn(),
      close: vi.fn(),
    };
    bootstrap = webSettingsBootstrap("skills");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(latestController?.state.settings.activeTab).toBe("skills");
  });

  it("switches web routes without reinitializing the App Server connection", async () => {
    const initialize = vi.fn(async () => ({ snapshot: clientSnapshot({ includeActiveTask: false }) }));
    const request = vi.fn(async (method: string) => {
      if (method === SETTINGS_GET_MCP_SERVERS) {
        return { generatedAt: "mcp-now", availability: "unavailable", servers: [] };
      }
      if (method === SETTINGS_GET_SKILLS) {
        return { generatedAt: "skills-now", availability: "unavailable", skills: [] };
      }
      if (method === SETTINGS_GET_AGENT_DETAILS) return { generatedAt: "now", agents: [] };
      return { revision: 1, tasks: [] };
    });
    const close = vi.fn();
    backendConnection = {
      initialize,
      request: request as unknown as BackendConnection["request"],
      respond: vi.fn(),
      close,
    };
    bootstrap = webTaskBootstrap();

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(latestController?.bootstrap.surface).toBe("task");

    await act(async () => {
      webRouteListeners[0]?.(webSettingsBootstrap());
      await Promise.resolve();
    });

    expect(latestController?.bootstrap.surface).toBe("settings");
    expect(initialize).toHaveBeenCalledTimes(1);
    expect(close).not.toHaveBeenCalled();
    expect(request).toHaveBeenCalledWith(SETTINGS_GET_AGENT_DETAILS, {});

    act(() => {
      webRouteListeners[0]?.(webTaskBootstrap(undefined, "project_2"));
    });

    expect(latestController?.bootstrap.surface).toBe("task");
    if (latestController?.bootstrap.surface !== "task") throw new Error("expected task bootstrap");
    expect(latestController?.bootstrap.taskId).toBeUndefined();
    expect(latestController?.bootstrap.projectId).toBe("project_2");
    expect(latestController?.state.newTask.selection.projectId).toBe("project_2");
    expect(latestController?.state.snapshot).toBeUndefined();
    expect(initialize).toHaveBeenCalledTimes(1);
    expect(close).not.toHaveBeenCalled();
  });

  it("loads new-task Agent options after a project route change even when an old task snapshot remains", async () => {
    const request = vi.fn(async (method: string) => {
      return { revision: 1, tasks: [] };
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot() })),
      request: request as unknown as BackendConnection["request"],
      respond: vi.fn(),
      close: vi.fn(),
    };
    bootstrap = webTaskBootstrap("task_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
    });

    act(() => {
      webRouteListeners[0]?.(webTaskBootstrap(undefined, "project_2"));
    });
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(latestController?.bootstrap.taskId).toBeUndefined();
    expect(latestController?.state.newTask.selection.projectId).toBe("project_2");
  });

  it("does not overwrite a user-selected Agent when initialize resolves late", async () => {
    const deferred = deferredInitialize();
    backendConnection = { initialize: vi.fn(() => deferred.promise), request: vi.fn(), respond: vi.fn(), close: vi.fn() };

    await act(async () => {
      create(<ControllerProbe />);
    });
    act(() => {
      latestController?.dispatch({ type: "newTask:agent", agentId: "opencode", agentLabel: "OpenCode" });
    });
    await act(async () => {
      deferred.resolve({
        snapshot: clientSnapshot({
          agents: [
            { agentId: "opencode" as never, label: "OpenCode", status: "disconnected" },
            { agentId: "custom.one" as never, label: "Custom One", status: "disconnected" },
          ],
          defaultAgentId: "custom.one",
        }),
      });
      await deferred.promise;
    });

    expect(latestController?.state.newTask.selection).toMatchObject({
      agentId: "opencode",
      agentLabel: "OpenCode",
    });
  });



  it("ignores legacy task snapshots while App Server initialize is pending", async () => {
    const deferred = deferredInitialize();
    backendConnection = { initialize: vi.fn(() => deferred.promise), request: vi.fn(), respond: vi.fn(), close: vi.fn() };
    bootstrap = taskBootstrap("task_1");

    await act(async () => {
      create(<ControllerProbe />);
    });
    const requestId = latestController?.createSnapshotRequestId("task_1", "open") ?? 0;
    act(() => {
      listeners[0]?.(legacyHostMessage({
        type: "task.snapshot",
        snapshot_intent: "open",
        snapshot_request_id: requestId,
        payload: snapshot("task_1", "inactive", "Fresh"),
      }));
    });

    await act(async () => {
      deferred.resolve({ snapshot: clientSnapshot({ activeTaskTitle: "Stale" }) });
      await deferred.promise;
    });

    expect(latestController?.state.snapshot?.task.title).toBe("Stale");
  });

  it("still ingests initialize state after a rejected legacy task snapshot", async () => {
    const deferred = deferredInitialize();
    backendConnection = { initialize: vi.fn(() => deferred.promise), request: vi.fn(), respond: vi.fn(), close: vi.fn() };
    bootstrap = taskBootstrap("task_1");

    await act(async () => {
      create(<ControllerProbe />);
    });
    act(() => {
      listeners[0]?.(legacyHostMessage({
        type: "task.snapshot",
        snapshot_intent: "open",
        snapshot_request_id: 999,
        payload: snapshot("task_1", "inactive", "Rejected"),
      }));
    });

    await act(async () => {
      deferred.resolve({ snapshot: clientSnapshot({ activeTaskTitle: "Initialize" }) });
      await deferred.promise;
    });

    expect(latestController?.state.snapshot?.task.title).toBe("Initialize");
  });

  it("still ingests initialize navigation after an ignored archive-mismatched legacy list", async () => {
    const deferred = deferredInitialize();
    backendConnection = { initialize: vi.fn(() => deferred.promise), request: vi.fn(), respond: vi.fn(), close: vi.fn() };

    await act(async () => {
      create(<ControllerProbe />);
    });
    act(() => {
      listeners[0]?.(legacyHostMessage({
        type: "task.list.result",
        payload: { archived: true, revision: 1, tasks: [snapshot("task_old", "inactive", "Ignored").task] },
      }));
    });

    await act(async () => {
      deferred.resolve({ snapshot: clientSnapshot({ activeTaskTitle: "Initialize" }) });
      await deferred.promise;
    });

    expect(latestController?.state.tasks.map((task) => task.title)).toEqual(["Initialize"]);
  });

  it("requests typed task list when initialize omits navigation tasks", async () => {
    const request = vi.fn(async () => ({
      revision: 4,
      tasks: [protocolTaskSummary("task_2", "Typed List")],
    }));
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ includeTasks: false, includeActiveTask: false }) })),
      request: request as unknown as BackendConnection["request"],
      respond: vi.fn(),
      close: vi.fn(),
    };

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(request).toHaveBeenCalledWith(TASK_LIST, { archived: false });
    expect(latestController?.state.tasks.map((task) => task.title)).toEqual(["Typed List"]);
  });

  it("starts archived web navigation in archive mode and requests archived tasks", async () => {
    const request = vi.fn(async () => ({
      archived: true,
      revision: 4,
      tasks: [protocolTaskSummary("task_archived", "Archived task")],
    }));
    bootstrap = navigationBootstrap({ archived: true });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ includeTasks: false, includeActiveTask: false }) })),
      request: request as unknown as BackendConnection["request"],
      respond: vi.fn(),
      close: vi.fn(),
    };

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(latestController?.state.showArchived).toBe(true);
    expect(request).toHaveBeenCalledWith(TASK_LIST, { archived: true });
    expect(latestController?.state.tasks.map((task) => task.title)).toEqual(["Archived task"]);
  });

  it("surfaces typed task-list failures without falling back to legacy task list", async () => {
    const request = vi.fn(async () => {
      throw new Error("Backend unavailable");
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ includeTasks: false, includeActiveTask: false }) })),
      request: request as unknown as BackendConnection["request"],
      respond: vi.fn(),
      close: vi.fn(),
    };

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(request).toHaveBeenCalledWith(TASK_LIST, { archived: false });
    expect(latestController?.state.taskListError).toBe("Backend unavailable");
    expect(postHostMessage).not.toHaveBeenCalledWith({ type: "task.list", payload: { archived: false } });
  });

  it("requests typed task open when initialize omits the active task", async () => {
    const request = vi.fn(async () => ({
      task: protocolTaskSnapshot("task_1", "Typed Open"),
    }));
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ includeActiveTask: false }) })),
      request: request as unknown as BackendConnection["request"],
      respond: vi.fn(),
      close: vi.fn(),
    };
    bootstrap = taskBootstrap("task_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(request).toHaveBeenCalledWith(TASK_OPEN, { taskId: "task_1" });
    expect(latestController?.state.snapshot?.task.title).toBe("Typed Open");
  });

  it("prepares a new Task with slash commands before the first send", async () => {
    const request = vi.fn(async (method: string) => {
      if (method === AGENT_LIST_SESSIONS) {
        return { agentId: "codex", projectLabel: "OpenAIDE", sessions: [], nextCursor: null };
      }
      if (method === TASK_CREATE) {
        return {
          task: {
            ...protocolTaskSnapshot("task_new", "New task", { hasMessages: false }),
            agentCommands: {
              state: "ready" as const,
              commands: [{ name: "review", description: "Review changes." }],
            },
          },
        };
      }
      throw new Error(method);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ includeActiveTask: false }) })),
      request: request as unknown as BackendConnection["request"],
      respond: vi.fn(),
      close: vi.fn(),
    };
    bootstrap = webTaskBootstrap(undefined, "project_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });

    expect({
      createCalls: request.mock.calls.filter(([method]) => method === TASK_CREATE),
      sendCalls: request.mock.calls.filter(([method]) => method === TASK_SEND),
      snapshot: latestController?.state.snapshot,
    }).toEqual({
      createCalls: [[TASK_CREATE, { projectId: "project_1", agentId: "codex" }]],
      sendCalls: [],
      snapshot: expect.objectContaining({
        task: expect.objectContaining({ task_id: "task_new", has_messages: false }),
        agent_commands: {
          agent_id: "codex",
          status: "ready",
          commands: [{ name: "review", description: "Review changes.", input_hint: undefined }],
        },
      }),
    });
  });

  it("refreshes slash commands when the prepared Task subscription becomes ready", async () => {
    const loadingTask = {
      ...protocolTaskSnapshot("task_new", "New task", { hasMessages: false }),
      agentCommands: { state: "loading" as const, commands: [] },
    };
    const readyTask = {
      ...loadingTask,
      revision: 2,
      agentCommands: {
        state: "ready" as const,
        commands: [{ name: "review", description: "Review changes." }],
      },
    };
    const request = vi.fn(async (method: string, params?: { scope?: { kind: string; taskId?: string } }) => {
      if (method === AGENT_LIST_SESSIONS) {
        return { agentId: "codex", projectLabel: "OpenAIDE", sessions: [], nextCursor: null };
      }
      if (method === TASK_CREATE) return { task: loadingTask };
      if (method === STATE_SUBSCRIBE) {
        const scope = params?.scope;
        if (scope?.kind === "projects") {
          return {
            cursor: "cursor_projects",
            scope,
            snapshot: {
              kind: "projects",
              projects: {
                activeProjectId: "project_1",
                projects: [{ projectId: "project_1", label: "OpenAIDE" }],
              },
            },
          };
        }
        if (scope?.kind === "agents") {
          return {
            cursor: "cursor_agents",
            scope,
            snapshot: {
              kind: "agents",
              agents: {
                defaultAgentId: "codex",
                agents: [{ agentId: "codex", label: "Codex", status: "connected" }],
              },
            },
          };
        }
        if (scope?.kind === "taskNavigation") {
          return {
            cursor: "cursor_navigation",
            scope,
            snapshot: { kind: "taskNavigation", navigation: { tasks: [], activeTaskId: null } },
          };
        }
        if (scope?.kind === "task" && scope.taskId === "task_new") {
          return {
            cursor: "cursor_task",
            scope,
            snapshot: { kind: "task", task: readyTask },
          };
        }
      }
      throw new Error(method);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ includeActiveTask: false }) })),
      request: request as unknown as BackendConnection["request"],
      events: vi.fn(() => vi.fn()),
      respond: vi.fn(),
      close: vi.fn(),
    };
    bootstrap = webTaskBootstrap(undefined, "project_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(request).toHaveBeenCalledWith(STATE_SUBSCRIBE, {
      scope: { kind: "task", taskId: "task_new" },
    });
    expect(latestController?.state.snapshot?.agent_commands).toEqual({
      agent_id: "codex",
      status: "ready",
      commands: [{ name: "review", description: "Review changes.", input_hint: undefined }],
    });
  });

  it("keeps the same prepared Task when config options change during preparation", async () => {
    const created = deferredValue<{ task: ReturnType<typeof protocolTaskSnapshot> }>();
    const request = vi.fn(async (method: string, params?: { taskId?: string }) => {
      if (method === AGENT_LIST_SESSIONS) {
        return { agentId: "codex", projectLabel: "OpenAIDE", sessions: [], nextCursor: null };
      }
      if (method === TASK_CREATE) {
        if (request.mock.calls.filter(([calledMethod]) => calledMethod === TASK_CREATE).length === 1) {
          return created.promise;
        }
        return { task: protocolTaskSnapshot("task_replaced", "New task", { hasMessages: false }) };
      }
      if (method === TASK_DISCARD) {
        return { discardedTaskId: params?.taskId, tasks: { tasks: [], activeTaskId: null } };
      }
      throw new Error(method);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ includeActiveTask: false }) })),
      request: request as unknown as BackendConnection["request"],
      respond: vi.fn(),
      close: vi.fn(),
    };
    bootstrap = webTaskBootstrap(undefined, "project_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(request.mock.calls.filter(([method]) => method === TASK_CREATE)).toHaveLength(1);

    act(() => {
      latestController?.dispatch({
        type: "newTask:configOptions:result",
        catalog: {
          agent_id: "codex",
          status: "ready",
          options: [{
            id: "model",
            label: "Model",
            category: "model",
            current_value: "gpt-5.5",
            values: [{ id: "gpt-5.5", label: "gpt-5.5" }],
          }],
        },
      });
    });

    await act(async () => {
      created.resolve({ task: protocolTaskSnapshot("task_prepared", "New task", { hasMessages: false }) });
      await created.promise;
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(request.mock.calls.filter(([method]) => method === TASK_CREATE)).toEqual([
      [TASK_CREATE, { projectId: "project_1", agentId: "codex" }],
    ]);
    expect(request.mock.calls.filter(([method]) => method === TASK_DISCARD)).toEqual([]);
    expect(request.mock.calls.filter(([method]) => method === TASK_SEND)).toEqual([]);
    expect(latestController?.state.snapshot?.task.task_id).toBe("task_prepared");
  });

  it("discards its prepared empty Task when routing to an existing Task before send", async () => {
    const request = vi.fn(async (method: string, params?: { taskId?: string }) => {
      if (method === AGENT_LIST_SESSIONS) {
        return { agentId: "codex", projectLabel: "OpenAIDE", sessions: [], nextCursor: null };
      }
      if (method === TASK_CREATE) {
        return { task: protocolTaskSnapshot("task_prepared", "New task", { hasMessages: false }) };
      }
      if (method === TASK_OPEN && params?.taskId === "task_existing") {
        return { task: protocolTaskSnapshot("task_existing", "Existing task") };
      }
      if (method === TASK_DISCARD) {
        return { discardedTaskId: params?.taskId, tasks: { tasks: [], activeTaskId: null } };
      }
      throw new Error(`${method}:${params?.taskId ?? ""}`);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ includeActiveTask: false }) })),
      request: request as unknown as BackendConnection["request"],
      respond: vi.fn(),
      close: vi.fn(),
    };
    bootstrap = webTaskBootstrap(undefined, "project_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(latestController?.state.snapshot?.task).toMatchObject({
      task_id: "task_prepared",
      has_messages: false,
    });

    await act(async () => {
      webRouteListeners.forEach((listener) => listener(webTaskBootstrap("task_existing")));
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(request).toHaveBeenCalledWith(TASK_OPEN, { taskId: "task_existing" });
    expect(request.mock.calls.filter(([method]) => method === TASK_DISCARD)).toEqual([
      [TASK_DISCARD, { taskId: "task_prepared" }],
    ]);
  });

  it("discards its prepared empty Task when the new-task surface unmounts", async () => {
    const request = vi.fn(async (method: string, params?: { taskId?: string }) => {
      if (method === AGENT_LIST_SESSIONS) {
        return { agentId: "codex", projectLabel: "OpenAIDE", sessions: [], nextCursor: null };
      }
      if (method === TASK_CREATE) {
        return { task: protocolTaskSnapshot("task_prepared", "New task", { hasMessages: false }) };
      }
      if (method === TASK_DISCARD) {
        return { discardedTaskId: params?.taskId, tasks: { tasks: [], activeTaskId: null } };
      }
      throw new Error(method);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ includeActiveTask: false }) })),
      request: request as unknown as BackendConnection["request"],
      respond: vi.fn(),
      close: vi.fn(),
    };
    bootstrap = webTaskBootstrap(undefined, "project_1");
    let renderer: ReturnType<typeof create> | undefined;

    await act(async () => {
      renderer = create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(latestController?.state.snapshot?.task.task_id).toBe("task_prepared");

    await act(async () => {
      renderer?.unmount();
      await Promise.resolve();
    });

    expect(request.mock.calls.filter(([method]) => method === TASK_DISCARD)).toEqual([
      [TASK_DISCARD, { taskId: "task_prepared" }],
    ]);
  });

  it("replaces the prepared empty Task when the selected Agent changes", async () => {
    const staleTaskOpen = deferredValue<never>();
    const request = vi.fn(async (method: string, params?: { agentId?: string; taskId?: string }) => {
      if (method === AGENT_LIST_SESSIONS) {
        return { agentId: params?.agentId, projectLabel: "OpenAIDE", sessions: [], nextCursor: null };
      }
      if (method === TASK_CREATE) {
        const agentId = params?.agentId ?? "codex";
        const prepared = protocolTaskSnapshot(`task_${agentId}`, "New task", { hasMessages: false });
        return {
          task: {
            ...prepared,
            task: { ...prepared.task, agentId },
            agentConfig: { ...prepared.agentConfig, agentId },
            agentCommands: { ...prepared.agentCommands, agentId },
          },
        };
      }
      if (method === TASK_DISCARD) {
        return { discardedTaskId: params?.taskId, tasks: { tasks: [], activeTaskId: null } };
      }
      if (method === TASK_OPEN) return staleTaskOpen.promise;
      throw new Error(method);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({
        snapshot: clientSnapshot({
          includeActiveTask: false,
          agents: [
            { agentId: "codex" as never, label: "Codex", status: "connected" },
            { agentId: "opencode" as never, label: "OpenCode", status: "connected" },
          ],
        }),
      })),
      request: request as unknown as BackendConnection["request"],
      respond: vi.fn(),
      close: vi.fn(),
    };
    bootstrap = webTaskBootstrap(undefined, "project_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(latestController?.state.snapshot?.task).toMatchObject({
      task_id: "task_codex",
      agent_id: "codex",
      has_messages: false,
    });
    request.mockClear();

    await act(async () => {
      latestController?.dispatch({ type: "newTask:agent", agentId: "opencode", agentLabel: "OpenCode" });
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(request.mock.calls.filter(([method]) => method === TASK_DISCARD)).toEqual([
      [TASK_DISCARD, { taskId: "task_codex" }],
    ]);
    expect(request.mock.calls.filter(([method]) => method === TASK_CREATE)).toEqual([
      [TASK_CREATE, { projectId: "project_1", agentId: "opencode" }],
    ]);
    expect(request.mock.calls.filter(([method]) => method === TASK_SEND)).toEqual([]);
    expect(latestController?.state.snapshot?.task).toMatchObject({
      task_id: "task_opencode",
      agent_id: "opencode",
      has_messages: false,
    });
  });

  it("does not show task-start submitting state while new-task options load", async () => {
    const request = vi.fn(async (method: string) => {
      if (method === TASK_CREATE) {
        return { task: protocolTaskSnapshot("task_prepared", "New task", { hasMessages: false }) };
      }
      throw new Error(method);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ includeActiveTask: false }) })),
      request: request as unknown as BackendConnection["request"],
      respond: vi.fn(),
      close: vi.fn(),
    };
    bootstrap = webTaskBootstrap(undefined, "project_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(request).toHaveBeenCalledWith(TASK_CREATE, {
      projectId: "project_1",
      agentId: "codex",
    });
    expect(latestController?.state.newTask.submitting).toBe(false);
    expect(request).not.toHaveBeenCalledWith(TASK_SEND, expect.anything());
  });

  it("keeps new-task typing when options finish after local draft edits", async () => {
    const options = deferredValue<unknown>();
    const request = vi.fn(async (method: string) => {
      throw new Error(method);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ includeActiveTask: false }) })),
      request: request as unknown as BackendConnection["request"],
      respond: vi.fn(),
      close: vi.fn(),
    };
    bootstrap = webTaskBootstrap(undefined, "project_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
    });
    act(() => {
      latestController?.dispatch({ type: "prompt", prompt: "keep this draft" });
    });
    await act(async () => {
      options.resolve(readyConfigOptions());
      await options.promise;
    });
    expect(latestController?.state.newTask.prompt).toBe("keep this draft");

    await act(async () => {
      vi.advanceTimersByTime(1000);
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(latestController?.state.newTask.prompt).toBe("keep this draft");
  });

  it("opens the created Task route after sending from new-task", async () => {
    const request = vi.fn(async (method: string) => {
      if (method === TASK_CREATE) return { task: protocolTaskSnapshot("task_new", "New task", { hasMessages: false }) };
      if (method === TASK_OPEN) return { task: protocolTaskSnapshot("task_new", "New task", { hasMessages: false }) };
      if (method === TASK_SEND) return { task: protocolTaskSnapshot("task_new", "Sent task", { hasMessages: true }) };
      throw new Error(method);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ includeActiveTask: false }) })),
      request: request as unknown as BackendConnection["request"],
      respond: vi.fn(),
      close: vi.fn(),
    };
    bootstrap = webTaskBootstrap(undefined, "project_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });
    await act(async () => {
      latestController?.dispatch({ type: "prompt", prompt: "ship it" });
      latestController?.dispatch({ type: "taskInput:prompt", taskId: "task_new", prompt: "ship it" });
      await Promise.resolve();
    });
    await act(async () => {
      latestController?.callbacks.newTask.submit();
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(request).toHaveBeenCalledWith(TASK_SEND, expect.objectContaining({
      taskId: "task_new",
      message: { text: "ship it" },
    }));
    expect(postHostMessage).toHaveBeenCalledWith({
      type: "surface.openTask",
      payload: {
        task_id: "task_new",
        title: "New task",
      },
    });
  });

  it("keeps sent follow-up text pending when the send response has no committed chat row", async () => {
    const request = vi.fn(async (method: string) => {
      if (method === TASK_SEND) {
        return { task: protocolTaskSnapshot("task_1", "Task", { hasMessages: true, status: "running" }) };
      }
      throw new Error(method);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot() })),
      request: request as unknown as BackendConnection["request"],
      respond: vi.fn(),
      close: vi.fn(),
    };
    bootstrap = taskBootstrap("task_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
    });
    await act(async () => {
      latestController?.dispatch({ type: "taskInput:prompt", taskId: "task_1", prompt: "do the work" });
      await Promise.resolve();
    });
    await act(async () => {
      latestController?.callbacks.task.sendPrompt();
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(request).toHaveBeenCalledWith(TASK_SEND, expect.objectContaining({
      taskId: "task_1",
      message: { text: "do the work" },
    }));
    expect(latestController?.state.taskInputs.task_1?.pending?.prompt).toBe("do the work");
  });

  it("sends a steering prompt immediately during an active turn", async () => {
    const request = vi.fn(async (method: string) => {
      if (method === TASK_SEND) {
        return { task: protocolTaskSnapshot("task_1", "Task", { hasMessages: true, status: "running" }) };
      }
      throw new Error(method);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ activeTaskStatus: "running" }) })),
      request: request as unknown as BackendConnection["request"],
      respond: vi.fn(),
      close: vi.fn(),
    };
    bootstrap = taskBootstrap("task_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
    });
    await act(async () => {
      latestController?.dispatch({ type: "taskInput:prompt", taskId: "task_1", prompt: "steer now" });
      await Promise.resolve();
    });
    await act(async () => {
      latestController?.callbacks.task.sendPrompt();
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(request).toHaveBeenCalledWith(TASK_SEND, expect.objectContaining({
      taskId: "task_1",
      message: { text: "steer now" },
    }));
    expect(latestController?.state.taskInputs.task_1?.pending?.prompt).toBe("steer now");
  });

  it("ignores a stale prepared task open response after new-task send commits messages", async () => {
    const routeOpen = deferredValue<{ task: ReturnType<typeof protocolTaskSnapshot> }>();
    const sent = deferredValue<{ task: ReturnType<typeof protocolTaskSnapshot> }>();
    const request = vi.fn((method: string) => {
      if (method === AGENT_LIST_SESSIONS) {
        return Promise.resolve({ agentId: "codex", projectLabel: "OpenAIDE", sessions: [], nextCursor: null });
      }
      if (method === TASK_CREATE) {
        return Promise.resolve({ task: protocolTaskSnapshot("task_new", "New task", { hasMessages: false }) });
      }
      if (method === TASK_OPEN) return routeOpen.promise;
      if (method === TASK_SEND) return sent.promise;
      throw new Error(method);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ includeActiveTask: false }) })),
      request: request as unknown as BackendConnection["request"],
      respond: vi.fn(),
      close: vi.fn(),
    };
    bootstrap = webTaskBootstrap(undefined, "project_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });
    await act(async () => {
      latestController?.dispatch({ type: "prompt", prompt: "ship it" });
      await Promise.resolve();
    });
    const submit = latestController?.callbacks.newTask.submit({ prompt: "ship it", context: [] });
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(postHostMessage).toHaveBeenCalledWith(expect.objectContaining({
      type: "surface.openTask",
      payload: expect.objectContaining({ task_id: "task_new" }),
    }));

    await act(async () => {
      webRouteListeners.forEach((listener) => listener(webTaskBootstrap("task_new")));
      await Promise.resolve();
    });
    expect(request).toHaveBeenCalledWith(TASK_OPEN, { taskId: "task_new" });

    await act(async () => {
      sent.resolve({ task: protocolTaskSnapshot("task_new", "Sent task", { hasMessages: true }) });
      await sent.promise;
      await submit;
    });
    expect(latestController?.state.snapshot?.task.title).toBe("Sent task");
    expect(latestController?.state.snapshot?.task.has_messages).toBe(true);

    await act(async () => {
      routeOpen.resolve({ task: protocolTaskSnapshot("task_new", "Stale prepared task", { hasMessages: false }) });
      await routeOpen.promise;
      await Promise.resolve();
    });

    expect(latestController?.state.snapshot?.task.title).toBe("Sent task");
    expect(latestController?.state.snapshot?.task.has_messages).toBe(true);
    expect(postHostMessage).not.toHaveBeenCalledWith(expect.objectContaining({ type: "surface.openNewTask" }));
  });

  it("reconciles a new task send that commits after switching to another task", async () => {
    const sent = deferredValue<{ task: ReturnType<typeof protocolTaskSnapshot> }>();
    const request = vi.fn((method: string, params?: { taskId?: string }) => {
      if (method === AGENT_LIST_SESSIONS) {
        return Promise.resolve({ agentId: "codex", projectLabel: "OpenAIDE", sessions: [], nextCursor: null });
      }
      if (method === TASK_CREATE) {
        return Promise.resolve({ task: protocolTaskSnapshot("task_new", "New task", { hasMessages: false }) });
      }
      if (method === TASK_OPEN && params?.taskId === "task_1") {
        return Promise.resolve({ task: protocolTaskSnapshot("task_1", "Previous task") });
      }
      if (method === TASK_SEND) return sent.promise;
      throw new Error(`${method}:${params?.taskId ?? ""}`);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ includeActiveTask: false }) })),
      request: request as unknown as BackendConnection["request"],
      respond: vi.fn(),
      close: vi.fn(),
    };
    bootstrap = webTaskBootstrap(undefined, "project_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });
    act(() => {
      latestController?.dispatch({ type: "prompt", prompt: "ship it" });
    });
    const submit = latestController?.callbacks.newTask.submit({ prompt: "ship it", context: [] });
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });

    await act(async () => {
      webRouteListeners.forEach((listener) => listener(webTaskBootstrap("task_1")));
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(latestController?.state.activeTaskId).toBe("task_1");
    expect(latestController?.visibleTasks.map((task) => task.task_id)).toContain("task_new");

    await act(async () => {
      sent.resolve({
        task: protocolTaskSnapshot("task_new", "Sent task", {
          hasMessages: true,
          status: "running",
          userText: "ship it",
        }),
      });
      await sent.promise;
      await submit;
    });

    expect(latestController?.state.activeTaskId).toBe("task_1");
    expect(latestController?.state.snapshot?.task.title).toBe("Previous task");
    expect(latestController?.state.tasks.find((task) => task.task_id === "task_new")).toMatchObject({
      has_messages: true,
      title: "Sent task",
    });
    expect(latestController?.state.taskInputs.task_new?.pending).toBeUndefined();
  });

  it("does not reopen a new Task when preparation resolves after switching away", async () => {
    const created = deferredValue<{ task: ReturnType<typeof protocolTaskSnapshot> }>();
    const request = vi.fn((method: string, params?: { taskId?: string }) => {
      if (method === AGENT_LIST_SESSIONS) {
        return Promise.resolve({ agentId: "codex", projectLabel: "OpenAIDE", sessions: [], nextCursor: null });
      }
      if (method === TASK_CREATE) return created.promise;
      if (method === TASK_OPEN && params?.taskId === "task_1") {
        return Promise.resolve({ task: protocolTaskSnapshot("task_1", "Previous task") });
      }
      if (method === TASK_SEND) {
        return Promise.resolve({ task: protocolTaskSnapshot("task_new", "Sent task", { hasMessages: true }) });
      }
      throw new Error(`${method}:${params?.taskId ?? ""}`);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ includeActiveTask: false }) })),
      request: request as unknown as BackendConnection["request"],
      respond: vi.fn(),
      close: vi.fn(),
    };
    bootstrap = webTaskBootstrap(undefined, "project_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
    });
    act(() => {
      latestController?.dispatch({ type: "prompt", prompt: "ship it" });
      latestController?.callbacks.newTask.submit({ prompt: "ship it", context: [] });
    });
    await act(async () => {
      latestController?.callbacks.navigation.openTask("task_1");
      webRouteListeners.forEach((listener) => listener(webTaskBootstrap("task_1")));
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(latestController?.state.activeTaskId).toBe("task_1");

    await act(async () => {
      created.resolve({ task: protocolTaskSnapshot("task_new", "New task", { hasMessages: false }) });
      await created.promise;
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(latestController?.state.activeTaskId).toBe("task_1");
    expect(latestController?.state.snapshot?.task.title).toBe("Previous task");
    expect(postHostMessage).not.toHaveBeenCalledWith(expect.objectContaining({
      type: "surface.openTask",
      payload: expect.objectContaining({ task_id: "task_new" }),
    }));
  });

  it("opens a fresh new-task surface while the previous new task is still starting", async () => {
    const sent = deferredValue<{ task: ReturnType<typeof protocolTaskSnapshot> }>();
    let createCount = 0;
    const request = vi.fn((method: string) => {
      if (method === AGENT_LIST_SESSIONS) {
        return Promise.resolve({ agentId: "codex", projectLabel: "OpenAIDE", sessions: [], nextCursor: null });
      }
      if (method === TASK_CREATE) {
        createCount += 1;
        const taskId = createCount === 1 ? "task_new" : "task_fresh";
        return Promise.resolve({ task: protocolTaskSnapshot(taskId, "New task", { hasMessages: false }) });
      }
      if (method === TASK_SEND) return sent.promise;
      throw new Error(method);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ includeActiveTask: false }) })),
      request: request as unknown as BackendConnection["request"],
      respond: vi.fn(),
      close: vi.fn(),
    };
    bootstrap = webTaskBootstrap(undefined, "project_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });
    act(() => {
      latestController?.dispatch({ type: "prompt", prompt: "ship it" });
    });
    const submit = latestController?.callbacks.newTask.submit({ prompt: "ship it", context: [] });
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(latestController?.state.taskInputs.task_new?.pending?.prompt).toBe("ship it");

    act(() => {
      latestController?.callbacks.navigation.openNewTask();
    });
    await act(async () => {
      webRouteListeners.forEach((listener) => listener(webTaskBootstrap(undefined, "project_1")));
      await Promise.resolve();
    });

    expect(latestController?.state.activeTaskId).toBe("task_fresh");
    expect(latestController?.state.newTask).toMatchObject({
      pending: undefined,
      prompt: "",
      submitting: false,
    });
    expect(latestController?.state.taskInputs.task_new?.pending?.prompt).toBe("ship it");
    expect(latestController?.visibleTasks.find((task) => task.task_id === "task_new")).toMatchObject({
      status: "active",
      title: "ship it",
    });

    await act(async () => {
      sent.resolve({
        task: protocolTaskSnapshot("task_new", "Sent task", {
          hasMessages: true,
          status: "running",
          userText: "ship it",
        }),
      });
      await sent.promise;
      await submit;
    });

    expect(latestController?.state.activeTaskId).toBe("task_fresh");
    expect(latestController?.state.tasks.find((task) => task.task_id === "task_new")).toMatchObject({
      has_messages: true,
      title: "Sent task",
    });
  });

  it("ignores stale Agent option failures after pasted image preparation opens the new Task", async () => {
    const options = deferredValue<unknown>();
    const request = vi.fn(async (method: string) => {
      if (method === TASK_CREATE) return { task: protocolTaskSnapshot("task_new", "New task") };
      if (method === ATTACHMENT_CREATE_PASTED_IMAGE) {
        return { attachment: { handleId: "attachment-handle-image", label: "pasted.png" } };
      }
      throw new Error(method);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ includeActiveTask: false }) })),
      request: request as unknown as BackendConnection["request"],
      respond: vi.fn(),
      close: vi.fn(),
    };
    bootstrap = webTaskBootstrap(undefined, "project_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
    });

    await act(async () => {
      await latestController?.callbacks.newTask.fileBrowser?.attachPastedImage(
        new File([new Uint8Array([1, 2, 3])], "pasted.png", { type: "image/png" }),
      );
    });
    await act(async () => {
      options.reject(new Error("options unavailable"));
      await options.promise.catch(() => undefined);
    });

    expect(latestController?.state.snapshot?.task.task_id).toBe("task_new");
    expect(latestController?.state.newTask.configOptionsError).toBeUndefined();
    expect(latestController?.state.taskInputs.task_new?.context[0]).toMatchObject({
      label: "pasted.png",
      app_server_handle_id: "attachment-handle-image",
    });
  });

  it("reuses the in-flight prepared Task when uploading an image", async () => {
    const created = deferredValue<{ task: ReturnType<typeof protocolTaskSnapshot> }>();
    const request = vi.fn(async (method: string) => {
      if (method === TASK_CREATE) return created.promise;
      if (method === TASK_OPEN) return { task: protocolTaskSnapshot("task_new", "New task", { hasMessages: false, sendReady: false }) };
      if (method === ATTACHMENT_CREATE_PASTED_IMAGE) {
        return { attachment: { handleId: "attachment-handle-image", label: "pasted.png" } };
      }
      throw new Error(method);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ includeActiveTask: false }) })),
      request: request as unknown as BackendConnection["request"],
      respond: vi.fn(),
      close: vi.fn(),
    };
    bootstrap = webTaskBootstrap(undefined, "project_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(request.mock.calls.filter(([method]) => method === TASK_CREATE)).toHaveLength(1);

    const upload = latestController?.callbacks.newTask.fileBrowser?.attachPastedImage(
      new File([new Uint8Array([1, 2, 3])], "pasted.png", { type: "image/png" }),
    );
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(request.mock.calls.filter(([method]) => method === TASK_CREATE)).toHaveLength(1);

    await act(async () => {
      created.resolve({ task: protocolTaskSnapshot("task_new", "New task", { hasMessages: false, sendReady: false }) });
      await created.promise;
      await upload;
    });

    expect(request.mock.calls.filter(([method]) => method === TASK_CREATE)).toHaveLength(1);
    expect(request).toHaveBeenCalledWith(ATTACHMENT_CREATE_PASTED_IMAGE, expect.objectContaining({
      taskId: "task_new",
    }));
    expect(latestController?.state.taskInputs.task_new?.context).toHaveLength(1);
  });

  it("keeps the draft and marks attachments for reselection when their handles are lost", async () => {
    const request = vi.fn(async (method: string) => {
      if (method === ATTACHMENT_CREATE_PASTED_IMAGE) {
        return { attachment: { handleId: "attachment-handle-image", label: "pasted.png" } };
      }
      if (method === TASK_SEND) {
        throw new AppServerProtocolError({
          error: {
            code: "attachmentHandleInvalid",
            message: "Attachment is no longer available. Reselect it and try again.",
            recoverable: true,
            target: { field: "attachments" },
          },
        });
      }
      throw new Error(method);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot() })),
      request: request as unknown as BackendConnection["request"],
      respond: vi.fn(),
      close: vi.fn(),
    };
    bootstrap = taskBootstrap("task_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
    });
    await act(async () => {
      await latestController?.callbacks.task.fileBrowser?.attachPastedImage(
        new File([new Uint8Array([1, 2, 3])], "pasted.png", { type: "image/png" }),
      );
    });
    await act(async () => {
      latestController?.callbacks.task.sendPrompt("Keep this draft");
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(latestController?.state.taskInputs.task_1).toEqual({
      prompt: "Keep this draft",
      context: [{
        kind: "file",
        label: "pasted.png",
        local_id: expect.any(String),
        preview_url: "data:image/png;base64,AQID",
        validation_error: "Attachment is no longer available. Reselect it and try again.",
      }],
      error: "Attachment is no longer available. Reselect it and try again.",
    });
  });

  it("ignores stale task-open responses after switching to another task", async () => {
    const task1 = deferredValue<{ task: ReturnType<typeof protocolTaskSnapshot> }>();
    const task2 = deferredValue<{ task: ReturnType<typeof protocolTaskSnapshot> }>();
    const request = vi.fn((method: string, params: { taskId?: string }) => {
      if (method === TASK_OPEN && params.taskId === "task_1") return task1.promise;
      if (method === TASK_OPEN && params.taskId === "task_2") return task2.promise;
      throw new Error(method);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ includeActiveTask: false }) })),
      request: request as unknown as BackendConnection["request"],
      respond: vi.fn(),
      close: vi.fn(),
    };
    bootstrap = webTaskBootstrap("task_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(request).toHaveBeenCalledWith(TASK_OPEN, { taskId: "task_1" });

    await act(async () => {
      webRouteListeners.forEach((listener) => listener(webTaskBootstrap("task_2")));
      await Promise.resolve();
    });
    expect(latestController?.state.activeTaskId).toBe("task_2");
    expect(latestController?.state.snapshot).toBeUndefined();
    expect(request).toHaveBeenCalledWith(TASK_OPEN, { taskId: "task_2" });

    await act(async () => {
      task2.resolve({ task: protocolTaskSnapshot("task_2", "Second task") });
      await task2.promise;
    });
    expect(latestController?.state.activeTaskId).toBe("task_2");
    expect(latestController?.state.snapshot?.task.title).toBe("Second task");

    await act(async () => {
      task1.resolve({ task: protocolTaskSnapshot("task_1", "Stale first task") });
      await task1.promise;
    });
    expect(latestController?.state.activeTaskId).toBe("task_2");
    expect(latestController?.state.snapshot?.task.title).toBe("Second task");
  });

  it("shows a cached task snapshot immediately when switching back while refreshing it", async () => {
    const task2 = deferredValue<{ task: ReturnType<typeof protocolTaskSnapshot> }>();
    const request = vi.fn((method: string, params: { taskId?: string }) => {
      if (method === TASK_OPEN && params.taskId === "task_1") {
        return Promise.resolve({ task: protocolTaskSnapshot("task_1", "Cached first task") });
      }
      if (method === TASK_OPEN && params.taskId === "task_2") return task2.promise;
      throw new Error(method);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ includeActiveTask: false }) })),
      request: request as unknown as BackendConnection["request"],
      respond: vi.fn(),
      close: vi.fn(),
    };
    bootstrap = webTaskBootstrap("task_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(latestController?.state.snapshot?.task.title).toBe("Cached first task");

    await act(async () => {
      webRouteListeners.forEach((listener) => listener(webTaskBootstrap("task_2")));
      await Promise.resolve();
    });
    expect(latestController?.state.activeTaskId).toBe("task_2");
    expect(latestController?.state.snapshot).toBeUndefined();

    await act(async () => {
      webRouteListeners.forEach((listener) => listener(webTaskBootstrap("task_1")));
      await Promise.resolve();
    });

    expect(latestController?.state.activeTaskId).toBe("task_1");
    expect(latestController?.state.snapshot?.task.title).toBe("Cached first task");
    expect(request).toHaveBeenCalledWith(TASK_OPEN, { taskId: "task_1" });
    expect(request.mock.calls.filter(([method, params]) =>
      method === TASK_OPEN && (params as { taskId?: string }).taskId === "task_1"
    )).toHaveLength(2);
  });

  it("waits for typed task open while backend initialize is pending", async () => {
    const initialize = deferredInitialize();
    const request = vi.fn(async () => ({ task: protocolTaskSnapshot("task_1", "Typed Open") }));
    backendConnection = {
      initialize: vi.fn(() => initialize.promise),
      request: request as unknown as BackendConnection["request"],
      respond: vi.fn(),
      close: vi.fn(),
    };
    bootstrap = taskBootstrap("task_1");

    await act(async () => {
      create(<ControllerProbe />);
    });
    await act(async () => {
      await Promise.resolve();
    });

    expect(request).not.toHaveBeenCalled();
    expect(postHostMessage).not.toHaveBeenCalledWith(expect.objectContaining({ type: "task.snapshot" }));
    expect(latestController?.state.taskOpenError).toBeUndefined();
  });

  it("does not reroute a pending no-message web task back to new task", async () => {
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ includeTasks: false, includeActiveTask: false }) })),
      request: vi.fn(),
      respond: vi.fn(),
      close: vi.fn(),
    };
    bootstrap = webTaskBootstrap("task_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
    });
    const pendingSnapshot = snapshot("task_1", "inactive");
    act(() => {
      latestController?.dispatch({ type: "taskInput:prompt", taskId: "task_1", prompt: "Build the thing" });
      latestController?.dispatch({ type: "taskInput:submit", taskId: "task_1" });
      latestController?.dispatch({
        type: "snapshot",
        intent: "open",
        snapshot: {
          ...pendingSnapshot,
          task: { ...pendingSnapshot.task, has_messages: false },
          chat: { ...pendingSnapshot.chat, has_messages: false, total_count: 0 },
        },
      });
    });
    await act(async () => {
      await Promise.resolve();
    });

    expect(postHostMessage).not.toHaveBeenCalledWith(expect.objectContaining({ type: "surface.openNewTask" }));
  });

  it("keeps an active no-message web task route after reload", async () => {
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ includeTasks: false, includeActiveTask: false }) })),
      request: vi.fn(),
      respond: vi.fn(),
      close: vi.fn(),
    };
    bootstrap = webTaskBootstrap("task_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
    });
    const activeSnapshot = snapshot("task_1", "active");
    act(() => {
      latestController?.dispatch({
        type: "snapshot",
        intent: "open",
        snapshot: {
          ...activeSnapshot,
          task: { ...activeSnapshot.task, has_messages: false },
          chat: { ...activeSnapshot.chat, has_messages: false, total_count: 0 },
        },
      });
    });
    await act(async () => {
      await Promise.resolve();
    });

    expect(postHostMessage).not.toHaveBeenCalledWith(expect.objectContaining({ type: "surface.openNewTask" }));
  });

  it("retries same-client pending new-task submission after reloading its task route", async () => {
    const request = vi.fn(async (method: string) => {
      if (method === TASK_OPEN) {
        return { task: protocolTaskSnapshot("task_1", "New task", { hasMessages: false }) };
      }
      if (method === TASK_SEND) {
        return { task: protocolTaskSnapshot("task_1", "Sent task", { hasMessages: true }) };
      }
      throw new Error(method);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ includeTasks: false, includeActiveTask: false }) })),
      request: request as unknown as BackendConnection["request"],
      respond: vi.fn(),
      close: vi.fn(),
    };
    bootstrap = webTaskBootstrap("task_1");
    sessionStorage.setItem("openaide:pending-task-send:v1", JSON.stringify({
      taskId: "task_1",
      taskRevision: 1,
      idempotencyKey: "frontend-send-reload-1",
      message: { text: "ship it" },
      renderState: { prompt: "ship it", context: [] },
    }));

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(request).toHaveBeenCalledWith(TASK_SEND, {
      taskId: "task_1",
      taskRevision: 1,
      idempotencyKey: "frontend-send-reload-1",
      message: { text: "ship it" },
    });
    expect(latestController?.state.snapshot?.task.has_messages).toBe(true);
    expect(sessionStorage.getItem("openaide:pending-task-send:v1")).toBeNull();
    expect(postHostMessage).not.toHaveBeenCalledWith(expect.objectContaining({ type: "surface.openNewTask" }));
  });

  it("waits for initialize before loading new-task Agent options", async () => {
    const initialize = deferredInitialize();
    const request = vi.fn();
    backendConnection = {
      initialize: vi.fn(() => initialize.promise),
      request: request as unknown as BackendConnection["request"],
      respond: vi.fn(),
      close: vi.fn(),
    };
    bootstrap = webTaskBootstrap(undefined, "project_1");

    await act(async () => {
      create(<ControllerProbe />);
    });
    await act(async () => {
      await Promise.resolve();
    });

    expect(request).not.toHaveBeenCalled();
  });

  it("does not post initialize failure fallback after unmount", async () => {
    const initialize = deferredInitialize();
    backendConnection = {
      initialize: vi.fn(() => initialize.promise),
      request: vi.fn() as unknown as BackendConnection["request"],
      respond: vi.fn(),
      close: vi.fn(),
    };
    bootstrap = taskBootstrap("task_1");

    let mounted: ReturnType<typeof create>;
    await act(async () => {
      mounted = create(<ControllerProbe />);
    });
    postHostMessage.mockClear();
    act(() => {
      mounted.unmount();
    });
    await act(async () => {
      initialize.reject(new Error("closed"));
      await initialize.promise.catch(() => undefined);
    });

    expect(postHostMessage).not.toHaveBeenCalledWith(expect.objectContaining({ type: "task.markRead" }));
  });

  it("ignores delayed typed startup list after archive generation changes", async () => {
    const taskList = deferredValue<{ revision: number; tasks: ReturnType<typeof protocolTaskSummary>[] }>();
    const archiveList = deferredValue<{ revision: number; tasks: ReturnType<typeof protocolTaskSummary>[] }>();
    const request = vi.fn()
      .mockImplementationOnce(() => taskList.promise)
      .mockImplementationOnce(() => archiveList.promise);
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ includeTasks: false, includeActiveTask: false }) })),
      request: request as unknown as BackendConnection["request"],
      respond: vi.fn(),
      close: vi.fn(),
    };

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
    });
    act(() => {
      latestController?.callbacks.navigation.toggleArchived();
    });
    await act(async () => {
      taskList.resolve({ revision: 4, tasks: [protocolTaskSummary("task_2", "Typed List")] });
      await taskList.promise;
    });

    expect(latestController?.state.showArchived).toBe(true);
    expect(latestController?.state.tasks).toEqual([]);
  });

  it("ignores initialize task navigation after archive generation changes", async () => {
    const initialize = deferredInitialize();
    backendConnection = {
      initialize: vi.fn(() => initialize.promise),
      request: vi.fn() as unknown as BackendConnection["request"],
      respond: vi.fn(),
      close: vi.fn(),
    };

    await act(async () => {
      create(<ControllerProbe />);
    });
    act(() => {
      latestController?.callbacks.navigation.toggleArchived();
    });
    await act(async () => {
      initialize.resolve({ snapshot: clientSnapshot({ activeTaskTitle: "Default List" }) });
      await initialize.promise;
    });

    expect(latestController?.state.showArchived).toBe(true);
    expect(latestController?.state.tasks).toEqual([]);
  });

  it("reports task-open error without BackendConnection", () => {
    bootstrap = taskBootstrap("task_1");
    act(() => {
      create(<ControllerProbe />);
    });

    expect(latestController?.state.taskOpenError).toEqual({
      taskId: "task_1",
      message: "App Server connection unavailable.",
    });
    expect(postHostMessage).not.toHaveBeenCalledWith(expect.objectContaining({ type: "task.snapshot" }));
  });

  it("emits task_rendered telemetry for initialized active tasks without polling refresh snapshots", async () => {
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ activeTaskStatus: "running" }) })),
      request: vi.fn() as unknown as BackendConnection["request"],
      respond: vi.fn(),
      close: vi.fn(),
    };
    bootstrap = taskBootstrap("task_1");
    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
    });

    expect(postHostMessage).toHaveBeenCalledWith({
      type: "webview.telemetry",
      payload: expect.objectContaining({
        event: "task_rendered",
        surface: "task",
        task_id: "task_1",
        task_status: "active",
        chat_items: 0,
      }),
    });
    postHostMessage.mockClear();

    act(() => {
      vi.advanceTimersByTime(1200);
    });
    expect(postHostMessage).not.toHaveBeenCalledWith(expect.objectContaining({ type: "task.snapshot" }));
  });
});

type TestBootstrap =
  | ReturnType<typeof navigationBootstrap>
  | ReturnType<typeof taskBootstrap>
  | ReturnType<typeof settingsBootstrap>
  | ReturnType<typeof webTaskBootstrap>
  | ReturnType<typeof webSettingsBootstrap>;

type TestBackendConnection = {
  initialize: (params: InitializeParams) => Promise<InitializeResult>;
  request: BackendConnection["request"];
  events?: BackendConnection["events"];
  respond: () => void;
  close: () => void;
};

function navigationBootstrap(options: { archived?: boolean } = {}) {
  return {
    surface: "navigation" as const,
    archived: options.archived,
    agents: [],
    preferences: { composer_submit_shortcut: "mod_enter" as const },
  };
}

function taskBootstrap(taskId: string) {
  return {
    surface: "task" as const,
    taskId,
    agents: [],
    preferences: { composer_submit_shortcut: "mod_enter" as const },
  };
}

function settingsBootstrap() {
  return {
    surface: "settings" as const,
    agents: [],
    preferences: { composer_submit_shortcut: "mod_enter" as const },
  };
}

function webTaskBootstrap(taskId?: string, projectId?: string) {
  return {
    surface: "task" as const,
    taskId,
    projectId,
    agents: [],
    preferences: { composer_submit_shortcut: "mod_enter" as const },
    appServerConnection: {
      kind: "webProxy" as const,
      endpointUrl: "/__openaide-app-server/probe",
    },
  };
}

function webSettingsBootstrap(settingsTab?: "agents" | "mcp" | "skills" | "common") {
  return {
    surface: "settings" as const,
    settingsTab,
    agents: [],
    preferences: { composer_submit_shortcut: "mod_enter" as const },
    appServerConnection: {
      kind: "webProxy" as const,
      endpointUrl: "/__openaide-app-server/probe",
    },
  };
}

function snapshot(taskId: string, status: TaskSnapshot["task"]["status"], title = "Task"): TaskSnapshot {
  return {
    task: {
      task_id: taskId,
      title,
      status,
      task_version: 1,
      message_history_version: 1,
      has_messages: true,
      unread: false,
      created_at: "2026-05-22T00:00:00.000Z",
      updated_at: "2026-05-22T00:00:00.000Z",
      last_activity: "2026-05-22T00:00:00.000Z",
      agent_id: "codex",
      agent_name: "Codex",
      isolation: "local",
      workspace_root: "/workspace",
    },
    chat: {
      task_id: taskId,
      items: [],
      has_before: false,
      has_messages: true,
      total_count: 0,
      version: 1,
    },
    permissions: [],
    send_capability: { state: "ready", attachment_only: true },
    settings_summary: {
      agent_id: "codex",
      isolation: "local",
      config_options: {},
    },
    revision: 1,
  };
}

function clientSnapshot(
  options: {
    activeTaskTitle?: string;
    activeTaskStatus?: "idle" | "running";
    agents?: NonNullable<ClientSnapshot["agents"]>["agents"];
    defaultAgentId?: string;
    includeTasks?: boolean;
    includeActiveTask?: boolean;
    appPreferences?: NonNullable<ClientSnapshot["settings"]>["preferences"];
    runtimeSettings?: NonNullable<ClientSnapshot["settings"]>["runtime"];
    settingsSections?: NonNullable<ClientSnapshot["settings"]>["sections"];
  } = {},
): ClientSnapshot {
  const includeTasks = options.includeTasks ?? true;
  const includeActiveTask = options.includeActiveTask ?? true;
  return {
    cursor: "cursor_1" as never,
    server: {
      serverId: "server_1" as never,
      protocolVersion: { major: 1, minor: 0 },
    },
    stateRoot: { stateRootId: "state_root_1" as never },
    client: {
      clientInstanceId: "client_1" as never,
      shellKind: "vscodeExtension",
      surface: { kind: "home" },
    },
    agents: {
      defaultAgentId: (options.defaultAgentId ?? "codex") as never,
      agents: options.agents ?? [{ agentId: "codex" as never, label: "Codex", status: "connected" }],
    },
    settings: {
      sections: options.settingsSections ?? [],
      preferences: options.appPreferences ?? null,
      runtime: options.runtimeSettings ?? null,
    },
    tasks: includeTasks ? {
      activeTaskId: "task_1" as never,
      tasks: [protocolTaskSummary("task_1", options.activeTaskTitle ?? "Task", options.activeTaskStatus)],
    } : null,
    activeTask: includeActiveTask
      ? protocolTaskSnapshot("task_1", options.activeTaskTitle ?? "Task", options.activeTaskStatus)
      : null,
  };
}

function protocolTaskSummary(taskId: string, title: string, status: "idle" | "running" = "idle", hasMessages = true) {
  return {
    taskId: taskId as never,
    projectId: "project_1" as never,
    agentId: "codex" as never,
    title,
    status,
    updatedAt: "2026-05-22T00:00:00.000Z",
    lastActivity: "2026-05-22T00:00:00.000Z",
    unread: false,
    hasMessages,
  };
}

function protocolTaskSnapshot(
  taskId: string,
  title: string,
  options: "idle" | "running" | {
    hasMessages?: boolean;
    sendReady?: boolean;
    status?: "idle" | "running";
    userText?: string;
  } = "idle",
) {
  const status = typeof options === "string" ? options : options.status ?? "idle";
  const hasMessages = typeof options === "string" ? true : options.hasMessages ?? true;
  const sendReady = typeof options === "string" ? true : options.sendReady ?? true;
  const userText = typeof options === "string" ? undefined : options.userText;
  return {
    task: protocolTaskSummary(taskId, title, status, hasMessages),
    revision: 1,
    preparation: { kind: "ready" as const },
    agentConfig: { state: "ready" as const, options: [] },
    agentCommands: { state: "ready" as const, commands: [] },
    sendCapability: { state: sendReady ? "ready" as const : "loading" as const },
    chat: {
      items: userText
        ? [{
            messageId: "user-1" as never,
            role: "user" as const,
            status: "complete" as const,
            parts: [{ kind: "text" as const, text: userText }],
          }]
        : [],
      hasMoreBefore: false,
      hasMessages,
    },
  };
}

function readyConfigOptions() {
  return {
    catalog: {
      agentId: "codex",
      status: "ready",
      options: [],
    },
  };
}

function deferredInitialize() {
  return deferredValue<InitializeResult>();
}

function deferredValue<T>() {
  let resolve!: (result: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

function memoryStorage(): Storage {
  const items = new Map<string, string>();
  return {
    get length() {
      return items.size;
    },
    clear: () => items.clear(),
    getItem: (key) => items.get(key) ?? null,
    key: (index) => Array.from(items.keys())[index] ?? null,
    removeItem: (key) => items.delete(key),
    setItem: (key, value) => items.set(key, value),
  };
}
