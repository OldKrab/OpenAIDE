import { act, create } from "react-test-renderer";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  AGENT_LIST_SESSIONS,
  ATTACHMENT_CREATE_PASTED_IMAGE,
  ATTACHMENT_RELEASE,
  AppServerProtocolError,
  createAppServerSession,
  PERMISSION_REQUEST,
  SETTINGS_GET_AGENT_DETAILS,
  SETTINGS_GET_MCP_SERVERS,
  SETTINGS_GET_SKILLS,
  STATE_SUBSCRIBE,
  STATE_UNSUBSCRIBE,
  TASK_ACQUIRE,
  TASK_RELEASE,
  TASK_LIST,
  TASK_MARK_READ,
  TASK_OPEN,
  TASK_SEND,
  TASK_SET_CONFIG_OPTION,
  type AppServerEvent,
  type BackendConnection,
  type BackendRecoveryBaseline,
  type ClientSnapshot,
  type InitializeParams,
  type InitializeResult,
  type ServerRequestMethod,
  type TaskSnapshot as ProtocolTaskSnapshot,
  type TypedServerRequest,
} from "@openaide/app-server-client";
import type { HostToWebviewMessage, TaskSnapshot } from "@openaide/app-shell-contracts";
import { projectIdForWorkspaceRoot } from "../state/projectIdentity";
import {
  type AppController,
  type AppControllerTestHarness,
  useAppController,
  useAppControllerTestHarness,
} from "./appController";

const postHostMessage = vi.fn();
const replaceSettingsTabRoute = vi.fn();
const listeners: Array<(message: HostToWebviewMessage) => void> = [];
const webRouteListeners: Array<(nextBootstrap: TestBootstrap) => void> = [];
let bootstrap: TestBootstrap = navigationBootstrap();
let backendConnection: TestBackendConnection | undefined;
let latestController: AppControllerTestHarness | undefined;
let latestPublicController: AppController | undefined;
const defaultHandleNotification: BackendConnection["handleNotification"] = () => () => undefined;
const defaultHandleRequest: BackendConnection["handleRequest"] = () => () => undefined;
const defaultHandleGenerationInvalidated: BackendConnection["handleGenerationInvalidated"] = () => () => undefined;
const defaultHandleRecoveryBaseline: BackendConnection["handleRecoveryBaseline"] = () => () => undefined;
const defaultHandleRecoveryFailed: BackendConnection["handleRecoveryFailed"] = () => () => undefined;

vi.mock("../services/hostBridge", () => ({
  getBackendConnection: () => {
    if (!backendConnection) return undefined;
    const connection = backendConnection;
    const request = connection.handleNotification
      ? connection.request
      : ((method, params, meta) => {
          // Most controller fixtures predate state subscriptions and keep their
          // background scopes inert so behavior-specific request spies stay focused.
          if (method === STATE_SUBSCRIBE) return new Promise(() => undefined);
          if (method === STATE_UNSUBSCRIBE) {
            return Promise.resolve({ scope: (params as { scope: unknown }).scope });
          }
          return meta === undefined
            ? connection.request(method, params)
            : connection.request(method, params, meta);
        }) as BackendConnection["request"];
    return createAppServerSession({
      ...connection,
      request,
      handleNotification: connection.handleNotification ?? defaultHandleNotification,
      handleRequest: defaultHandleRequest,
      handleGenerationInvalidated: connection.handleGenerationInvalidated
        ?? defaultHandleGenerationInvalidated,
      handleRecoveryBaseline: connection.handleRecoveryBaseline ?? defaultHandleRecoveryBaseline,
      handleRecoveryFailed: defaultHandleRecoveryFailed,
    });
  },
  getBootstrap: () => bootstrap,
  openNewTaskSurface: (projectId?: string) => postHostMessage(projectId
    ? { type: "surface.openNewTask", payload: { project_id: projectId } }
    : { type: "surface.openNewTask" }),
  openSettingsSurface: () => postHostMessage({ type: "surface.openSettings" }),
  openTaskSurface: (taskId: string, title?: string) => postHostMessage({
    type: "surface.openTask",
    payload: { task_id: taskId, ...(title ? { title } : {}) },
  }),
  postHostMessage: (message: unknown) => postHostMessage(message),
  replaceSettingsTabRoute: (tab: unknown) => replaceSettingsTabRoute(tab),
  subscribeSurfaceRouteChanges: (listener: (nextBootstrap: TestBootstrap) => void) => {
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
  latestController = useAppControllerTestHarness();
  return null;
}

function PublicControllerProbe() {
  latestPublicController = useAppController();
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
    replaceSettingsTabRoute.mockClear();
    listeners.length = 0;
    webRouteListeners.length = 0;
    bootstrap = navigationBootstrap();
    backendConnection = undefined;
    latestController = undefined;
    latestPublicController = undefined;
  });

  it("exposes render-ready surface state and intents without reducer controls", () => {
    act(() => {
      create(<PublicControllerProbe />);
    });

    expect(latestPublicController).not.toHaveProperty("state");
    expect(latestPublicController).not.toHaveProperty("dispatch");
    expect(latestPublicController).not.toHaveProperty("createSnapshotRequestId");

    act(() => {
      latestPublicController?.intents.newTask.changePrompt("Describe the work");
    });
    expect(latestPublicController?.view.primaryTask.newTask.newTask.prompt).toBe("Describe the work");
  });

  it("keeps the default backend connection stable across public controller renders", () => {
    const initialize = vi.fn(() => new Promise<InitializeResult>(() => undefined));
    const close = vi.fn();
    backendConnection = { initialize, request: vi.fn(), close };
    bootstrap = navigationBootstrap();
    let mounted!: ReturnType<typeof create>;

    act(() => {
      mounted = create(<PublicControllerProbe />);
    });
    act(() => {
      latestPublicController?.intents.newTask.changePrompt("Trigger a render");
    });

    expect(initialize).toHaveBeenCalledOnce();
    expect(close).not.toHaveBeenCalled();
    act(() => mounted.unmount());
    expect(close).toHaveBeenCalledOnce();
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
    backendConnection = { initialize, request: vi.fn(), close };
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

  it("loads one Native Session page without eager pagination on startup", async () => {
    bootstrap = navigationBootstrap({ projectId: "project_1" });
    const request = vi.fn(async (method: string, params?: { cursor?: string | null }) => {
      if (method !== AGENT_LIST_SESSIONS) throw new Error(method);
      const start = params?.cursor ? 3 : 1;
      const count = params?.cursor ? 12 : 2;
      return {
        agentId: "codex",
        projectId: "project_1",
        projectLabel: "OpenAIDE",
        sessions: Array.from({ length: count }, (_, index) => ({
          sessionId: `native_${start + index}`,
          title: `Native ${start + index}`,
        })),
        nextCursor: params?.cursor ? null : "cursor_2",
      };
    });
    const initializedSnapshot = clientSnapshot({ includeActiveTask: false });
    initializedSnapshot.client.surface = { kind: "project", projectId: "project_1" as never };
    initializedSnapshot.newTaskDefaults.projectId = "project_1" as never;
    initializedSnapshot.projects = {
      projects: [{ projectId: "project_1" as never, label: "OpenAIDE", workspaceRoot: "/workspace/OpenAIDE", available: true }],
    };
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: initializedSnapshot })),
      request: request as unknown as BackendConnection["request"],
      close: vi.fn(),
    };

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(request.mock.calls.filter(([method]) => method === AGENT_LIST_SESSIONS)).toEqual([
      [AGENT_LIST_SESSIONS, { agentId: "codex", projectId: "project_1", cursor: null }],
    ]);
    expect(latestController?.state.newTask.nativeSessions.items).toHaveLength(2);
    expect(latestController?.state.newTask.nativeSessions.nextCursor).toBe("cursor_2");
  });

  it("finishes refreshing native sessions after opening a task", async () => {
    bootstrap = navigationBootstrap({ projectId: "project_1" });
    const sessionList = deferredValue<{
      agentId: string;
      projectId: string;
      projectLabel: string;
      sessions: Array<{ sessionId: string; title: string }>;
      nextCursor: null;
    }>();
    const request = vi.fn(async (method: string) => {
      if (method === AGENT_LIST_SESSIONS) return sessionList.promise;
      if (method === TASK_OPEN) return { task: protocolTaskSnapshot("task_1", "Opened Task") };
      throw new Error(method);
    });
    const initializedSnapshot = clientSnapshot({ includeActiveTask: false });
    initializedSnapshot.client.surface = { kind: "project", projectId: "project_1" as never };
    initializedSnapshot.newTaskDefaults.projectId = "project_1" as never;
    initializedSnapshot.projects = {
      projects: [{ projectId: "project_1" as never, label: "OpenAIDE", workspaceRoot: "/workspace/OpenAIDE", available: true }],
    };
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: initializedSnapshot })),
      request: request as unknown as BackendConnection["request"],
      close: vi.fn(),
    };

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(latestController?.state.newTask.nativeSessions.loading).toBe(true);

    await act(async () => {
      latestController?.callbacks.navigation.openTask("task_1");
      webRouteListeners.forEach((listener) => listener(taskBootstrap("task_1")));
      await Promise.resolve();
      sessionList.resolve({
        agentId: "codex",
        projectId: "project_1",
        projectLabel: "OpenAIDE",
        sessions: [{ sessionId: "native_1", title: "Native session" }],
        nextCursor: null,
      });
      await sessionList.promise;
      await Promise.resolve();
    });

    expect(latestController?.state.newTask.nativeSessions).toMatchObject({
      loading: false,
      items: [{ session_id: "native_1", title: "Native session" }],
    });
  });

  it("settles an initialization failure as unavailable", async () => {
    backendConnection = {
      initialize: vi.fn(async () => {
        throw new Error("App Server request timed out.");
      }),
      request: vi.fn() as unknown as BackendConnection["request"],
      close: vi.fn(),
    };
    bootstrap = taskBootstrap("task_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
    });

    expect(latestController?.backendConnectionState).toEqual({
      status: "unavailable",
      message: "App Server request timed out.",
    });
    expect(latestController?.state.appServerError).toBe("App Server request timed out.");
    expect(latestController?.backendReady).toBe(false);
  });

  it("keeps a completed open task unread until the user returns attention", async () => {
    const windowEvents = new EventTarget();
    const documentEvents = new EventTarget();
    vi.stubGlobal("window", Object.assign(windowEvents, {
      clearTimeout: globalThis.clearTimeout,
      setTimeout: globalThis.setTimeout,
    }));
    vi.stubGlobal("document", Object.assign(documentEvents, { visibilityState: "visible" }));
    const initial = clientSnapshot();
    if (!initial.activeTask) throw new Error("expected active task fixture");
    initial.activeTask.task.unread = true;
    const acknowledged = protocolTaskSnapshot("task_1", "Task");
    acknowledged.task.unread = false;
    acknowledged.revision = 2;
    const request = vi.fn(async (method: string, _params?: unknown) => {
      if (method === TASK_MARK_READ) return { task: acknowledged };
      throw new Error(method);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: initial })),
      request: request as unknown as BackendConnection["request"],
      close: vi.fn(),
    };
    bootstrap = taskBootstrap("task_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
    });

    expect(request).not.toHaveBeenCalledWith(TASK_MARK_READ, expect.anything());

    await act(async () => {
      windowEvents.dispatchEvent(new Event("pointerdown"));
      await Promise.resolve();
    });

    expect(request).toHaveBeenCalledWith(TASK_MARK_READ, { taskId: "task_1" });
    expect(latestController?.state.snapshot?.task.unread).toBe(false);
  });

  it("uses App Server Agent snapshots for new-task Agent selection", async () => {
    backendConnection = {
      initialize: vi.fn(async () => ({
        snapshot: clientSnapshot({
          agents: [
            { agentId: "opencode" as never, label: "OpenCode", status: "disconnected" },
            { agentId: "custom.one" as never, label: "Custom One", status: "disconnected" },
          ],
          newTaskDefaultAgentId: "custom.one",
        }),
      })),
      request: vi.fn(),
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
    backendConnection = { initialize: vi.fn(() => deferred.promise), request: vi.fn(), close: vi.fn() };

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
          newTaskDefaultAgentId: "custom.one",
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
    backendConnection = { initialize: vi.fn(() => deferred.promise), request: vi.fn(), close: vi.fn() };
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
    backendConnection = { initialize: vi.fn(() => deferred.promise), request: vi.fn(), close: vi.fn() };
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
    backendConnection = { initialize: vi.fn(() => deferred.promise), request: vi.fn(), close: vi.fn() };

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
      close: vi.fn(),
    };
    bootstrap = taskBootstrap("task_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(request).toHaveBeenCalledTimes(1);
    expect(request).toHaveBeenCalledWith(TASK_OPEN, { taskId: "task_1" });
    expect(latestController?.state.snapshot?.task.title).toBe("Typed Open");
  });

  it("keeps terminal history sync when its event wins the race with task open", async () => {
    const initial = clientSnapshot();
    if (!initial.activeTask) throw new Error("expected active task fixture");
    initial.activeTask.historySync = { state: "idle", generation: 6 };
    const opened = deferredValue<{ task: ReturnType<typeof protocolTaskSnapshot> }>();
    const eventListeners: Array<(event: AppServerEvent) => void> = [];
    const request = vi.fn((method: string, params?: { scope?: { kind: string } }) => {
      if (method === TASK_OPEN) return opened.promise;
      if (method === STATE_UNSUBSCRIBE) return Promise.resolve({ scope: params?.scope });
      if (method === STATE_SUBSCRIBE) {
        if (params?.scope?.kind === "task") {
          return Promise.resolve(taskSubscriptionSnapshot("cursor-1", initial.activeTask!));
        }
        return Promise.resolve(nonTaskSubscriptionSnapshot(params?.scope, "cursor-1"));
      }
      throw new Error(method);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: initial })),
      request: request as unknown as BackendConnection["request"],
      handleNotification: (_method, listener) => {
        eventListeners.push(listener);
        return () => undefined;
      },
      close: vi.fn(),
    };
    bootstrap = taskBootstrap("task_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });

    const historyEvent = (
      previousCursor: string,
      cursor: string,
      historySync: ProtocolTaskSnapshot["historySync"],
    ): AppServerEvent => ({
      subscription: { kind: "task", taskId: "task_1" as never },
      previousCursor: previousCursor as never,
      cursor: cursor as never,
      scope: {
        kind: "task",
        stateRootId: "state_root_1" as never,
        taskId: "task_1" as never,
      },
      payload: {
        kind: "taskHistorySyncUpdated",
        taskId: "task_1" as never,
        historySync,
      },
    });
    await act(async () => {
      for (const listener of eventListeners) {
        listener(historyEvent("cursor-1", "cursor-2", { state: "syncing", generation: 7 }));
        listener(historyEvent("cursor-2", "cursor-3", { state: "idle", generation: 7 }));
      }
      await Promise.resolve();
    });
    expect(latestController?.state.snapshot?.history_sync).toEqual({ state: "idle", generation: 7 });

    const staleOpen = protocolTaskSnapshot("task_1", "Newer durable title");
    staleOpen.revision = 2;
    staleOpen.historySync = { state: "syncing", generation: 7 };
    await act(async () => {
      opened.resolve({ task: staleOpen });
      await opened.promise;
      await Promise.resolve();
    });

    expect(latestController?.state.snapshot?.history_sync).toEqual({ state: "idle", generation: 7 });
    expect(latestController?.state.snapshot).toMatchObject({
      revision: 2,
      task: { title: "Newer durable title" },
    });
  });

  it("retries a failed task open in place without discarding the draft", async () => {
    let openAttempts = 0;
    const request = vi.fn(async (method: string) => {
      if (method !== TASK_OPEN) throw new Error(method);
      openAttempts += 1;
      if (openAttempts === 1) throw new Error("Connection closed.");
      return { task: protocolTaskSnapshot("task_1", "Recovered Task") };
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot() })),
      request: request as unknown as BackendConnection["request"],
      close: vi.fn(),
    };
    bootstrap = taskBootstrap("task_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(latestController?.state.taskOpenError).toEqual({
      taskId: "task_1",
      message: "Connection closed.",
    });
    expect(latestController?.backendConnectionState).toEqual({
      status: "unavailable",
      message: "Connection closed.",
    });

    act(() => {
      latestController?.dispatch({ type: "taskInput:prompt", taskId: "task_1", prompt: "Keep this draft" });
    });
    await act(async () => {
      latestController?.retryTaskOpen();
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(request.mock.calls.filter(([method]) => method === TASK_OPEN)).toHaveLength(2);
    expect(latestController?.state.snapshot?.task.title).toBe("Recovered Task");
    expect(latestController?.state.taskInputs.task_1?.prompt).toBe("Keep this draft");
  });

  it("opens an initialized task route to recover unavailable Agent config", async () => {
    const initial = clientSnapshot();
    if (!initial.activeTask) throw new Error("expected active task fixture");
    initial.activeTask.agentConfig = { state: "unavailable" };

    const recovered = protocolTaskSnapshot("task_1", "Recovered Task");
    recovered.agentConfig = {
      state: "ready",
      options: [{
        configId: "model" as never,
        label: "Model",
        category: "model",
        kind: "select",
        currentValue: { type: "id", value: "gpt-5.6-sol" },
        values: [{ value: "gpt-5.6-sol", label: "GPT-5.6 Sol" }],
      }],
    };
    const opened = deferredValue<{ task: ReturnType<typeof protocolTaskSnapshot> }>();
    const request = vi.fn((method: string) => {
      if (method === TASK_OPEN) return opened.promise;
      throw new Error(method);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: initial })),
      request: request as unknown as BackendConnection["request"],
      close: vi.fn(),
    };
    bootstrap = taskBootstrap("task_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(request.mock.calls.filter(([method]) => method === TASK_OPEN)).toEqual([
      [TASK_OPEN, { taskId: "task_1" }],
    ]);
    expect(latestController?.state.snapshot).toMatchObject({
      task: { task_id: "task_1", title: "Task" },
      agent_config: { status: "unavailable", options: [] },
    });

    await act(async () => {
      opened.resolve({ task: recovered });
      await opened.promise;
    });

    expect(latestController?.state.snapshot?.agent_config).toMatchObject({
      status: "ready",
      options: [{ id: "model", current_value: { type: "id", value: "gpt-5.6-sol" } }],
    });
  });

  it("reports automatic Task subscription recovery without discarding the draft", async () => {
    let taskSubscriptionAttempts = 0;
    const request = vi.fn(async (
      method: string,
      params?: { scope?: { kind: string; taskId?: string } },
    ) => {
      if (method === TASK_OPEN) {
        return { task: protocolTaskSnapshot("task_1", "Task") };
      }
      if (method === STATE_UNSUBSCRIBE) return { scope: params?.scope };
      if (method !== STATE_SUBSCRIBE) throw new Error(method);
      if (params?.scope?.kind !== "task") {
        return nonTaskSubscriptionSnapshot(params?.scope, "cursor_1");
      }
      taskSubscriptionAttempts += 1;
      if (taskSubscriptionAttempts === 1) throw new Error("NetworkError when attempting to fetch resource.");
      return taskSubscriptionSnapshot("cursor_2");
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot() })),
      request: request as unknown as BackendConnection["request"],
      handleNotification: () => () => undefined,
      close: vi.fn(),
    };
    bootstrap = taskBootstrap("task_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(latestController?.backendConnectionState).toEqual({
      status: "reconnecting",
      message: "App Server is temporarily unavailable.",
    });
    expect(postHostMessage).toHaveBeenCalledWith({
      type: "webview.telemetry",
      payload: expect.objectContaining({
        event: "app_server_subscription_failed",
        request: expect.stringMatching(/^\d+:task_1$/),
        error_name: "Error",
      }),
    });
    expect(latestController?.backendReady).toBe(false);

    act(() => {
      latestController?.dispatch({ type: "taskInput:prompt", taskId: "task_1", prompt: "Keep this draft" });
    });
    await act(async () => {
      vi.advanceTimersByTime(500);
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(taskSubscriptionAttempts).toBe(2);
    expect(latestController?.backendConnectionState).toEqual({ status: "ready" });
    expect(latestController?.backendReady).toBe(true);
    expect(latestController?.state.taskInputs.task_1?.prompt).toBe("Keep this draft");
    expect(postHostMessage).toHaveBeenCalledWith({
      type: "webview.telemetry",
      payload: expect.objectContaining({
        event: "app_server_subscription_recovered",
        request: expect.stringMatching(/^\d+:task_1$/),
      }),
    });
    expect(JSON.stringify(postHostMessage.mock.calls)).not.toContain("NetworkError when attempting to fetch resource.");
  });

  it("opens a task once when navigation changes to its route", async () => {
    const request = vi.fn(async (method: string) => {
      if (method === TASK_OPEN) {
        return { task: protocolTaskSnapshot("task_1", "Opened Task") };
      }
      throw new Error(method);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot() })),
      request: request as unknown as BackendConnection["request"],
      close: vi.fn(),
    };
    bootstrap = navigationBootstrap();

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
    });
    await act(async () => {
      latestController?.callbacks.navigation.openTask("task_1");
      webRouteListeners.forEach((listener) => listener(webTaskBootstrap("task_1")));
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(request.mock.calls.filter(([method]) => method === TASK_OPEN)).toEqual([
      [TASK_OPEN, { taskId: "task_1" }],
    ]);
    expect(latestController?.state.snapshot?.task.title).toBe("Opened Task");
  });

  it("prepares a new Task with slash commands before the first send", async () => {
    const request = vi.fn(async (method: string) => {
      if (method === AGENT_LIST_SESSIONS) {
        return { agentId: "codex", projectLabel: "OpenAIDE", sessions: [], nextCursor: null };
      }
      if (method === TASK_ACQUIRE) {
        return {
          task: {
            ...protocolTaskSnapshot("task_new", "New task", { hasMessages: false }),
            lifecycle: "new" as const,
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
      createCalls: request.mock.calls.filter(([method]) => method === TASK_ACQUIRE),
      sendCalls: request.mock.calls.filter(([method]) => method === TASK_SEND),
      snapshot: latestController?.newTaskSnapshot,
    }).toEqual({
      createCalls: [[TASK_ACQUIRE, { projectId: "project_1", agentId: "codex" }]],
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

  it("acquires a new Task before loading optional Native Session history", async () => {
    const nativeSessions = deferredValue<{
      agentId: string;
      projectId: string;
      projectLabel: string;
      sessions: [];
      nextCursor: null;
    }>();
    const request = vi.fn(async (method: string) => {
      if (method === TASK_ACQUIRE) {
        return {
          task: {
            ...protocolTaskSnapshot("task_new", "New task", { hasMessages: false }),
            lifecycle: "new" as const,
          },
        };
      }
      if (method === AGENT_LIST_SESSIONS) return nativeSessions.promise;
      throw new Error(method);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ includeActiveTask: false }) })),
      request: request as unknown as BackendConnection["request"],
      close: vi.fn(),
    };
    bootstrap = webTaskBootstrap(undefined, "project_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });

    const startupRequests = request.mock.calls
      .map(([method]) => method)
      .filter((method) => method === TASK_ACQUIRE || method === AGENT_LIST_SESSIONS);
    expect(startupRequests).toEqual([TASK_ACQUIRE, AGENT_LIST_SESSIONS]);
    expect(latestController?.newTaskSnapshot?.task.task_id).toBe("task_new");

    await act(async () => {
      nativeSessions.resolve({
        agentId: "codex",
        projectId: "project_1",
        projectLabel: "OpenAIDE",
        sessions: [],
        nextCursor: null,
      });
      await nativeSessions.promise;
    });
  });

  it("keeps the hidden New Task subscription current while an existing Task is visible", async () => {
    const loadingTask = {
      ...protocolTaskSnapshot("task_new", "New task", { hasMessages: false }),
      lifecycle: "new" as const,
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
    const taskBaseline = deferredValue<{
      cursor: never;
      scope: { kind: "task"; taskId: never };
      snapshot: { kind: "task"; task: typeof readyTask };
    }>();
    const request = vi.fn(async (method: string, params?: { scope?: { kind: string; taskId?: string } }) => {
      if (method === AGENT_LIST_SESSIONS) {
        return { agentId: "codex", projectLabel: "OpenAIDE", sessions: [], nextCursor: null };
      }
      if (method === TASK_ACQUIRE) return { task: loadingTask };
      if (method === STATE_SUBSCRIBE) {
        const scope = params?.scope;
        if (scope?.kind === "projects") {
          return {
            cursor: "cursor_projects",
            scope,
            snapshot: {
              kind: "projects",
              projects: {
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
          return taskBaseline.promise;
        }
      }
      if (method === TASK_OPEN) {
        return { task: protocolTaskSnapshot("task_existing", "Existing task") };
      }
      throw new Error(method);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ includeActiveTask: false }) })),
      request: request as unknown as BackendConnection["request"],
      handleNotification: vi.fn(() => vi.fn()),
      close: vi.fn(),
    };
    bootstrap = webTaskBootstrap(undefined, "project_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(request).toHaveBeenCalledWith(STATE_SUBSCRIBE, {
      scope: { kind: "task", taskId: "task_new" },
    });
    await act(async () => {
      webRouteListeners.forEach((listener) => listener(webTaskBootstrap("task_existing")));
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(latestController?.state.snapshot?.task.task_id).toBe("task_existing");

    await act(async () => {
      taskBaseline.resolve({
        cursor: "cursor_task" as never,
        scope: { kind: "task", taskId: "task_new" as never },
        snapshot: { kind: "task", task: readyTask },
      });
      await taskBaseline.promise;
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(latestController?.state.snapshot?.task.task_id).toBe("task_existing");
    expect(latestController?.newTaskSnapshot?.agent_commands).toEqual({
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
      if (method === TASK_ACQUIRE) {
        if (request.mock.calls.filter(([calledMethod]) => calledMethod === TASK_ACQUIRE).length === 1) {
          return created.promise;
        }
        return { task: protocolTaskSnapshot("task_replaced", "New task", { hasMessages: false }) };
      }
      if (method === TASK_RELEASE) {
        return { discardedTaskId: params?.taskId };
      }
      throw new Error(method);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ includeActiveTask: false }) })),
      request: request as unknown as BackendConnection["request"],
      close: vi.fn(),
    };
    bootstrap = webTaskBootstrap(undefined, "project_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(request.mock.calls.filter(([method]) => method === TASK_ACQUIRE)).toHaveLength(1);

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
            kind: "select", current_value: { type: "id", value: "gpt-5.5" },
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

    expect(request.mock.calls.filter(([method]) => method === TASK_ACQUIRE)).toEqual([
      [TASK_ACQUIRE, { projectId: "project_1", agentId: "codex" }],
    ]);
    expect(request.mock.calls.filter(([method]) => method === TASK_RELEASE)).toEqual([]);
    expect(request.mock.calls.filter(([method]) => method === TASK_SEND)).toEqual([]);
    expect(latestController?.newTaskSnapshot?.task.task_id).toBe("task_prepared");
  });

  it("reuses the cached New Task with its composer after visiting an existing Task", async () => {
    const request = vi.fn(async (method: string, params?: { taskId?: string }) => {
      if (method === AGENT_LIST_SESSIONS) {
        return { agentId: "codex", projectLabel: "OpenAIDE", sessions: [], nextCursor: null };
      }
      if (method === TASK_ACQUIRE) {
        return {
          task: {
            ...protocolTaskSnapshot("task_prepared", "New task", { hasMessages: false }),
            lifecycle: "new" as const,
          },
        };
      }
      if (method === TASK_OPEN && params?.taskId === "task_existing") {
        return { task: protocolTaskSnapshot("task_existing", "Existing task") };
      }
      throw new Error(`${method}:${params?.taskId ?? ""}`);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ includeActiveTask: false }) })),
      request: request as unknown as BackendConnection["request"],
      close: vi.fn(),
    };
    bootstrap = webTaskBootstrap(undefined, "project_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(latestController?.newTaskSnapshot?.task).toMatchObject({
      task_id: "task_prepared",
      has_messages: false,
    });
    await act(async () => {
      latestController?.dispatch({ type: "taskInput:prompt", taskId: "task_prepared", prompt: "Keep this" });
      latestController?.dispatch({
        type: "taskInput:attachment:addAppServer",
        taskId: "task_prepared",
        attachment: {
          kind: "file",
          label: "notes.md",
          local_id: "attachment-1",
          app_server_handle_id: "handle-1" as never,
        },
      });
      await Promise.resolve();
    });

    await act(async () => {
      webRouteListeners.forEach((listener) => listener(webTaskBootstrap("task_existing")));
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(request).toHaveBeenCalledWith(TASK_OPEN, { taskId: "task_existing" });
    expect(latestController?.state.snapshot?.task.task_id).toBe("task_existing");

    await act(async () => {
      webRouteListeners.forEach((listener) => listener(webTaskBootstrap(undefined, "project_1")));
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(request.mock.calls.filter(([method]) => method === TASK_ACQUIRE)).toHaveLength(1);
    expect(request.mock.calls.filter(([method]) => method === TASK_RELEASE)).toEqual([]);
    expect(latestController?.newTaskSnapshot?.task.task_id).toBe("task_prepared");
    expect(latestController?.state.taskInputs.task_prepared).toMatchObject({
      prompt: "Keep this",
      context: [expect.objectContaining({ app_server_handle_id: "handle-1" })],
    });
    expect(latestController?.state.tasks.map((task) => task.task_id)).not.toContain("task_prepared");
  });

  it("reacquires an expired Prepared Task while preserving the Composer draft", async () => {
    let publishRecoveryBaseline: ((baseline: BackendRecoveryBaseline) => void) | undefined;
    let publishInvalidation: Parameters<BackendConnection["handleGenerationInvalidated"]>[0] | undefined;
    let acquireCount = 0;
    const request = vi.fn(async (method: string, params?: { scope?: { kind: string; taskId?: string } }) => {
      if (method === STATE_SUBSCRIBE && params?.scope) {
        if (params.scope.kind === "task" && params.scope.taskId) {
          return taskSubscriptionSnapshot(
            `cursor_${params.scope.taskId}`,
            protocolTaskSnapshot(params.scope.taskId, "New task", { hasMessages: false }),
          );
        }
        return nonTaskSubscriptionSnapshot(params.scope);
      }
      if (method === AGENT_LIST_SESSIONS) {
        return { agentId: "codex", projectLabel: "OpenAIDE", sessions: [], nextCursor: null };
      }
      if (method === TASK_ACQUIRE) {
        acquireCount += 1;
        return {
          task: protocolTaskSnapshot(
            acquireCount === 1 ? "task_expired" : "task_reacquired",
            "New task",
            { hasMessages: false },
          ),
        };
      }
      throw new Error(method);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ includeActiveTask: false }) })),
      request: request as unknown as BackendConnection["request"],
      handleNotification: defaultHandleNotification,
      handleGenerationInvalidated(handler) {
        publishInvalidation = handler;
        return () => {
          publishInvalidation = undefined;
        };
      },
      handleRecoveryBaseline(handler) {
        publishRecoveryBaseline = handler;
        return () => {
          publishRecoveryBaseline = undefined;
        };
      },
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
      latestController?.dispatch({ type: "prompt", prompt: "Keep this draft" });
    });
    expect(publishInvalidation).toBeTypeOf("function");
    expect(publishRecoveryBaseline).toBeTypeOf("function");

    await act(async () => {
      publishInvalidation?.({ reason: "clientLivenessExpired" });
      publishRecoveryBaseline?.({
        reason: "clientLivenessExpired",
        result: { snapshot: clientSnapshot({ includeActiveTask: false }) },
      });
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(latestController?.newTaskSnapshot?.task.task_id).toBe("task_reacquired");
    expect(request.mock.calls.filter(([method]) => method === TASK_ACQUIRE)).toHaveLength(2);
    expect(latestController?.state.newTask.prompt).toBe("Keep this draft");
  });

  it("keeps prepared Agent options visible while an expired lease is reacquired", async () => {
    let publishRecoveryBaseline: ((baseline: BackendRecoveryBaseline) => void) | undefined;
    let publishInvalidation: Parameters<BackendConnection["handleGenerationInvalidated"]>[0] | undefined;
    const reacquired = deferredValue<{ task: ReturnType<typeof protocolTaskSnapshot> }>();
    let acquireCount = 0;
    const preparedTask = (taskId: string) => {
      const task = protocolTaskSnapshot(taskId, "New task", { hasMessages: false });
      task.agentConfig = {
        state: "ready",
        options: [{
          configId: "model" as never,
          label: "Model",
          category: "model",
          kind: "select",
          currentValue: { type: "id", value: "gpt-5.6-sol" },
          values: [{ value: "gpt-5.6-sol", label: "GPT-5.6 Sol" }],
        }],
      };
      return task;
    };
    const request = vi.fn(async (method: string, params?: { scope?: { kind: string; taskId?: string } }) => {
      if (method === STATE_SUBSCRIBE && params?.scope) {
        if (params.scope.kind === "task" && params.scope.taskId) {
          return taskSubscriptionSnapshot(
            `cursor_${params.scope.taskId}`,
            preparedTask(params.scope.taskId),
          );
        }
        return nonTaskSubscriptionSnapshot(params.scope);
      }
      if (method === AGENT_LIST_SESSIONS) {
        return { agentId: "codex", projectLabel: "OpenAIDE", sessions: [], nextCursor: null };
      }
      if (method === TASK_ACQUIRE) {
        acquireCount += 1;
        return acquireCount === 1
          ? { task: preparedTask("task_expired") }
          : reacquired.promise;
      }
      throw new Error(method);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ includeActiveTask: false }) })),
      request: request as unknown as BackendConnection["request"],
      handleNotification: defaultHandleNotification,
      handleGenerationInvalidated(handler) {
        publishInvalidation = handler;
        return () => {
          publishInvalidation = undefined;
        };
      },
      handleRecoveryBaseline(handler) {
        publishRecoveryBaseline = handler;
        return () => {
          publishRecoveryBaseline = undefined;
        };
      },
      close: vi.fn(),
    };
    bootstrap = webTaskBootstrap(undefined, "project_1");

    await act(async () => {
      create(<PublicControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(latestPublicController?.view.primaryTask.newTask.snapshot?.agent_config?.options[0]?.label)
      .toBe("Model");

    await act(async () => {
      publishInvalidation?.({ reason: "clientLivenessExpired" });
      publishRecoveryBaseline?.({
        reason: "clientLivenessExpired",
        result: { snapshot: clientSnapshot({ includeActiveTask: false }) },
      });
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(request.mock.calls.filter(([method]) => method === TASK_ACQUIRE)).toHaveLength(2);
    expect(latestPublicController?.view.primaryTask.newTask.snapshot).toBeUndefined();
    expect(latestPublicController?.view.primaryTask.newTask.newTask.configOptions?.options[0]?.label)
      .toBe("Model");

    await act(async () => {
      reacquired.resolve({ task: preparedTask("task_reacquired") });
      await reacquired.promise;
      await Promise.resolve();
      await Promise.resolve();
    });
  });

  it("sends through the cached New Task after visiting an existing Task without reopening it", async () => {
    const request = vi.fn(async (method: string, params?: { taskId?: string }) => {
      if (method === AGENT_LIST_SESSIONS) {
        return { agentId: "codex", projectLabel: "OpenAIDE", sessions: [], nextCursor: null };
      }
      if (method === TASK_ACQUIRE) {
        return { task: protocolTaskSnapshot("task_prepared", "New task", { hasMessages: false }) };
      }
      if (method === TASK_OPEN && params?.taskId === "task_existing") {
        return { task: protocolTaskSnapshot("task_existing", "Existing task") };
      }
      if (method === TASK_SEND && params?.taskId === "task_prepared") {
        return {
          task: protocolTaskSnapshot("task_prepared", "New task", { userText: "Send this" }),
          turnId: "turn-1",
          userMessageId: "user-1",
        };
      }
      throw new Error(`${method}:${params?.taskId ?? ""}`);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ includeActiveTask: false }) })),
      request: request as unknown as BackendConnection["request"],
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
      latestController?.dispatch({ type: "taskInput:prompt", taskId: "task_prepared", prompt: "Send this" });
      await Promise.resolve();
    });
    await act(async () => {
      webRouteListeners.forEach((listener) => listener(webTaskBootstrap("task_existing")));
      await Promise.resolve();
      await Promise.resolve();
    });
    await act(async () => {
      webRouteListeners.forEach((listener) => listener(webTaskBootstrap(undefined, "project_1")));
      await Promise.resolve();
      await Promise.resolve();
    });
    await act(async () => {
      latestController?.callbacks.newTask.submit();
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(request.mock.calls.filter(([method]) => method === TASK_ACQUIRE)).toHaveLength(1);
    expect(request.mock.calls.filter(([method, params]) => (
      method === TASK_OPEN && (params as { taskId?: string }).taskId === "task_prepared"
    ))).toEqual([]);
    expect(request).toHaveBeenCalledWith(TASK_SEND, expect.objectContaining({
      taskId: "task_prepared",
      message: { text: "Send this" },
    }));
  });

  it("retains its prepared New Task for Backend reuse when the new-task surface unmounts", async () => {
    const request = vi.fn(async (method: string, params?: { taskId?: string }) => {
      if (method === AGENT_LIST_SESSIONS) {
        return { agentId: "codex", projectLabel: "OpenAIDE", sessions: [], nextCursor: null };
      }
      if (method === TASK_ACQUIRE) {
        return { task: protocolTaskSnapshot("task_prepared", "New task", { hasMessages: false }) };
      }
      if (method === TASK_RELEASE) {
        return { discardedTaskId: params?.taskId };
      }
      throw new Error(method);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ includeActiveTask: false }) })),
      request: request as unknown as BackendConnection["request"],
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
    expect(latestController?.newTaskSnapshot?.task.task_id).toBe("task_prepared");

    await act(async () => {
      renderer?.unmount();
      await Promise.resolve();
    });

    expect(request.mock.calls.filter(([method]) => method === TASK_RELEASE)).toEqual([]);
  });

  it("replaces the prepared empty Task when the selected Agent changes", async () => {
    const staleTaskOpen = deferredValue<never>();
    const request = vi.fn(async (method: string, params?: { agentId?: string; taskId?: string }) => {
      if (method === AGENT_LIST_SESSIONS) {
        return { agentId: params?.agentId, projectLabel: "OpenAIDE", sessions: [], nextCursor: null };
      }
      if (method === TASK_ACQUIRE) {
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
      if (method === TASK_RELEASE) {
        return { discardedTaskId: params?.taskId };
      }
      if (method === ATTACHMENT_RELEASE) {
        return { outcomes: [] };
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
      close: vi.fn(),
    };
    bootstrap = webTaskBootstrap(undefined, "project_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(latestController?.newTaskSnapshot?.task).toMatchObject({
      task_id: "task_codex",
      agent_id: "codex",
      has_messages: false,
    });
    await act(async () => {
      latestController?.dispatch({ type: "prompt", prompt: "keep this draft" });
      latestController?.dispatch({
        type: "newTask:attachment:add",
        attachment: {
          kind: "image",
          label: "old.png",
          payload: { data: "AQID", mimeType: "image/png" },
        },
      });
      await Promise.resolve();
    });
    request.mockClear();

    await act(async () => {
      latestController?.dispatch({ type: "newTask:agent", agentId: "opencode", agentLabel: "OpenCode" });
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(request.mock.calls.filter(([method]) => method === TASK_RELEASE)).toEqual([
      [TASK_RELEASE, { taskId: "task_codex" }],
    ]);
    expect(request.mock.calls.filter(([method]) => method === TASK_ACQUIRE)).toEqual([
      [TASK_ACQUIRE, { projectId: "project_1", agentId: "opencode" }],
    ]);
    expect(request.mock.calls.filter(([method]) => method === TASK_SEND)).toEqual([]);
    expect(request).not.toHaveBeenCalledWith(ATTACHMENT_RELEASE, expect.anything());
    expect(latestController?.state.taskInputs.task_codex).toBeUndefined();
    expect(latestController?.state.newTask).toMatchObject({
      prompt: "keep this draft",
      context: [{ label: "old.png", kind: "image" }],
    });
    expect(latestController?.newTaskSnapshot?.task).toMatchObject({
      task_id: "task_opencode",
      agent_id: "opencode",
      has_messages: false,
    });
  });

  it("does not show task-start submitting state while new-task options load", async () => {
    const request = vi.fn(async (method: string) => {
      if (method === TASK_ACQUIRE) {
        return { task: protocolTaskSnapshot("task_prepared", "New task", { hasMessages: false }) };
      }
      throw new Error(method);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ includeActiveTask: false }) })),
      request: request as unknown as BackendConnection["request"],
      close: vi.fn(),
    };
    bootstrap = webTaskBootstrap(undefined, "project_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(request).toHaveBeenCalledWith(TASK_ACQUIRE, {
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
      if (method === TASK_ACQUIRE) return { task: protocolTaskSnapshot("task_new", "New task", { hasMessages: false }) };
      if (method === TASK_OPEN) return { task: protocolTaskSnapshot("task_new", "New task", { hasMessages: false }) };
      if (method === TASK_SEND) return { task: protocolTaskSnapshot("task_new", "Sent task", { hasMessages: true }) };
      throw new Error(method);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ includeActiveTask: false }) })),
      request: request as unknown as BackendConnection["request"],
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
        title: "Sent task",
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

  it("sends a steering message during active Agent work", async () => {
    const request = vi.fn(async (method: string) => {
      if (method === TASK_SEND) {
        return {
          task: protocolTaskSnapshot("task_1", "Task", {
            hasMessages: true,
            status: "running",
            userText: "steer now",
          }),
          turnId: "turn-primary",
          userMessageId: "message-steer",
        };
      }
      throw new Error(method);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ activeTaskStatus: "running" }) })),
      request: request as unknown as BackendConnection["request"],
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

    expect(request).toHaveBeenCalledWith(TASK_SEND, {
      taskId: "task_1",
      message: { text: "steer now" },
    });
    expect(latestController?.state.taskInputs.task_1).toMatchObject({
      prompt: "",
      context: [],
    });
  });

  it("ignores a stale prepared task open response after new-task send commits messages", async () => {
    const routeOpen = deferredValue<{ task: ReturnType<typeof protocolTaskSnapshot> }>();
    const sent = deferredValue<{
      task: ReturnType<typeof protocolTaskSnapshot>;
      turnId: string;
      userMessageId: string;
    }>();
    const request = vi.fn((method: string) => {
      if (method === AGENT_LIST_SESSIONS) {
        return Promise.resolve({ agentId: "codex", projectLabel: "OpenAIDE", sessions: [], nextCursor: null });
      }
      if (method === TASK_ACQUIRE) {
        return Promise.resolve({ task: protocolTaskSnapshot("task_new", "New task", { hasMessages: false }) });
      }
      if (method === TASK_OPEN) return routeOpen.promise;
      if (method === TASK_SEND) return sent.promise;
      throw new Error(method);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ includeActiveTask: false }) })),
      request: request as unknown as BackendConnection["request"],
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
    expect(postHostMessage).not.toHaveBeenCalledWith(expect.objectContaining({ type: "surface.openTask" }));

    await act(async () => {
      sent.resolve({
        task: protocolTaskSnapshot("task_new", "Sent task", { hasMessages: true }),
        turnId: "turn-new",
        userMessageId: "user-new",
      });
      await sent.promise;
      await submit;
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
    const sent = deferredValue<{
      task: ReturnType<typeof protocolTaskSnapshot>;
      turnId: string;
      userMessageId: string;
    }>();
    const request = vi.fn((method: string, params?: { taskId?: string }) => {
      if (method === AGENT_LIST_SESSIONS) {
        return Promise.resolve({ agentId: "codex", projectLabel: "OpenAIDE", sessions: [], nextCursor: null });
      }
      if (method === TASK_ACQUIRE) {
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
    expect(latestController?.visibleTasks.map((task) => task.task_id)).not.toContain("task_new");

    await act(async () => {
      sent.resolve({
        turnId: "turn-new",
        userMessageId: "user-new",
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

  it("retains a late task/acquire result without replacing the visible Task", async () => {
    const created = deferredValue<{ task: ReturnType<typeof protocolTaskSnapshot> }>();
    const request = vi.fn((method: string, params?: { taskId?: string }) => {
      if (method === AGENT_LIST_SESSIONS) {
        return Promise.resolve({ agentId: "codex", projectLabel: "OpenAIDE", sessions: [], nextCursor: null });
      }
      if (method === TASK_ACQUIRE) return created.promise;
      if (method === TASK_OPEN && params?.taskId === "task_1") {
        return Promise.resolve({ task: protocolTaskSnapshot("task_1", "Destination task") });
      }
      if (method === TASK_SEND) {
        return Promise.resolve({
          task: protocolTaskSnapshot("task_new", "Sent task", { hasMessages: true }),
          turnId: "turn-new",
          userMessageId: "user-new",
        });
      }
      throw new Error(`${method}:${params?.taskId ?? ""}`);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ includeActiveTask: false }) })),
      request: request as unknown as BackendConnection["request"],
      close: vi.fn(),
    };
    bootstrap = webTaskBootstrap(undefined, "project_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
    });
    await act(async () => {
      webRouteListeners.forEach((listener) => listener(webTaskBootstrap("task_1")));
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(latestController?.state.snapshot?.task.title).toBe("Destination task");

    await act(async () => {
      created.resolve({
        task: {
          ...protocolTaskSnapshot("task_new", "New task", { hasMessages: false }),
          lifecycle: "new",
        },
      });
      await created.promise;
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(latestController?.state.activeTaskId).toBe("task_1");
    expect(latestController?.state.snapshot?.task.title).toBe("Destination task");
    expect(request.mock.calls.filter(([method]) => method === TASK_RELEASE)).toEqual([]);
    expect(latestController?.newTaskSnapshot?.task.task_id).toBe("task_new");
    expect(latestController?.state.tasks.map((task) => task.task_id)).not.toContain("task_new");
    expect(postHostMessage).not.toHaveBeenCalledWith(expect.objectContaining({
      type: "surface.openTask",
      payload: expect.objectContaining({ task_id: "task_new" }),
    }));
  });

  it("reopens the same New Task while its first send is unresolved", async () => {
    const sent = deferredValue<{ task: ReturnType<typeof protocolTaskSnapshot> }>();
    let createCount = 0;
    const request = vi.fn((method: string) => {
      if (method === AGENT_LIST_SESSIONS) {
        return Promise.resolve({ agentId: "codex", projectLabel: "OpenAIDE", sessions: [], nextCursor: null });
      }
      if (method === TASK_ACQUIRE) {
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

    expect(createCount).toBe(1);
    expect(latestController?.newTaskSnapshot?.task.task_id).toBe("task_new");
    expect(latestController?.state.newTask).toMatchObject({
      submitting: true,
    });
    expect(latestController?.state.taskInputs.task_new?.pending?.prompt).toBe("ship it");
    expect(latestController?.visibleTasks.find((task) => task.task_id === "task_new")).toBeUndefined();

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

    expect(latestController?.newTaskSnapshot).toBeUndefined();
    expect(latestController?.state.tasks.find((task) => task.task_id === "task_new")).toMatchObject({
      has_messages: true,
      title: "Sent task",
    });
  });

  it("prepares and exposes Agent config for a new draft after the previous task was accepted", async () => {
    let createCount = 0;
    const request = vi.fn(async (method: string) => {
      if (method === AGENT_LIST_SESSIONS) {
        return { agentId: "codex", projectLabel: "OpenAIDE", sessions: [], nextCursor: null };
      }
      if (method === TASK_ACQUIRE) {
        createCount += 1;
        const task = protocolTaskSnapshot(`task_${createCount}`, "New task", { hasMessages: false });
        if (createCount === 2) {
          task.agentConfig = {
            state: "ready",
            options: [{
              configId: "model" as never,
              label: "Model",
              category: "model",
              kind: "select",
              currentValue: { type: "id", value: "gpt-5" },
              values: [{ value: "gpt-5", label: "GPT-5" }],
            }],
          };
        }
        return {
          task,
        };
      }
      if (method === TASK_SEND) {
        return {
          task: protocolTaskSnapshot("task_1", "First task", {
            hasMessages: true,
            userText: "first message",
          }),
        };
      }
      if (method === TASK_OPEN) {
        return { task: protocolTaskSnapshot("task_1", "First task", { hasMessages: true }) };
      }
      throw new Error(method);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ includeActiveTask: false }) })),
      request: request as unknown as BackendConnection["request"],
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
      latestController?.callbacks.newTask.submit({ prompt: "first message", context: [] });
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });
    act(() => {
      webRouteListeners.forEach((listener) => listener(webTaskBootstrap("task_1", "project_1")));
    });
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
    await act(async () => {
      latestController?.callbacks.navigation.openNewTask();
      webRouteListeners.forEach((listener) => listener(webTaskBootstrap(undefined, "project_1")));
      for (let index = 0; index < 8; index += 1) await Promise.resolve();
    });

    expect(request.mock.calls.filter(([method]) => method === TASK_ACQUIRE)).toHaveLength(2);
    expect(latestController?.newTaskSnapshot).toMatchObject({
      task: { task_id: "task_2", has_messages: false },
      agent_config: {
        agent_id: "codex",
        status: "ready",
        options: [expect.objectContaining({
          id: "model",
          current_value: { type: "id", value: "gpt-5" },
        })],
      },
    });
  });

  it("ignores stale Agent option failures after pasted image preparation opens the new Task", async () => {
    const options = deferredValue<unknown>();
    const request = vi.fn(async (method: string) => {
      if (method === TASK_ACQUIRE) {
        return { task: protocolTaskSnapshot("task_new", "New task", { hasMessages: false }) };
      }
      throw new Error(method);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ includeActiveTask: false }) })),
      request: request as unknown as BackendConnection["request"],
      close: vi.fn(),
    };
    bootstrap = webTaskBootstrap(undefined, "project_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
    });

    await act(async () => {
      await latestController?.callbacks.newTask.fileBrowser?.attachImage(
        new File([new Uint8Array([1, 2, 3])], "pasted.png", { type: "image/png" }),
      );
    });
    await act(async () => {
      options.reject(new Error("options unavailable"));
      await options.promise.catch(() => undefined);
    });

    expect(latestController?.newTaskSnapshot?.task.task_id).toBe("task_new");
    expect(latestController?.state.newTask.configOptionsError).toBeUndefined();
    expect(latestController?.state.newTask.context[0]).toMatchObject({
      label: "pasted.png",
      kind: "image",
      payload: { data: "AQID", mimeType: "image/png" },
    });
  });

  it("keeps an Image local while prepared-Task acquisition is in flight", async () => {
    const created = deferredValue<{ task: ReturnType<typeof protocolTaskSnapshot> }>();
    const request = vi.fn(async (method: string) => {
      if (method === TASK_ACQUIRE) return created.promise;
      if (method === TASK_OPEN) return { task: protocolTaskSnapshot("task_new", "New task", { hasMessages: false, sendReady: false }) };
      throw new Error(method);
    });
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ includeActiveTask: false }) })),
      request: request as unknown as BackendConnection["request"],
      close: vi.fn(),
    };
    bootstrap = webTaskBootstrap(undefined, "project_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(request.mock.calls.filter(([method]) => method === TASK_ACQUIRE)).toHaveLength(1);

    const upload = latestController?.callbacks.newTask.fileBrowser?.attachImage(
      new File([new Uint8Array([1, 2, 3])], "pasted.png", { type: "image/png" }),
    );
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(request.mock.calls.filter(([method]) => method === TASK_ACQUIRE)).toHaveLength(1);

    await act(async () => {
      created.resolve({ task: protocolTaskSnapshot("task_new", "New task", { hasMessages: false, sendReady: false }) });
      await created.promise;
      await upload;
    });

    expect(request.mock.calls.filter(([method]) => method === TASK_ACQUIRE)).toHaveLength(1);
    expect(request).not.toHaveBeenCalledWith(ATTACHMENT_CREATE_PASTED_IMAGE, expect.anything());
    expect(latestController?.state.newTask.context).toHaveLength(1);
  });

  it("keeps the draft and marks attachments for reselection when their handles are lost", async () => {
    const request = vi.fn(async (method: string) => {
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
      close: vi.fn(),
    };
    bootstrap = taskBootstrap("task_1");

    await act(async () => {
      create(<ControllerProbe />);
      await Promise.resolve();
    });
    await act(async () => {
      await latestController?.callbacks.task.fileBrowser?.attachImage(
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
        kind: "image",
        label: "pasted.png",
        local_id: expect.any(String),
        preview_url: "data:image/png;base64,AQID",
        payload: { data: "AQID", mimeType: "image/png" },
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

  it("waits for initialize before loading new-task Agent options", async () => {
    const initialize = deferredInitialize();
    const request = vi.fn();
    backendConnection = {
      initialize: vi.fn(() => initialize.promise),
      request: request as unknown as BackendConnection["request"],
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
    const request = vi.fn();
    backendConnection = {
      initialize: vi.fn(async () => ({ snapshot: clientSnapshot({ activeTaskStatus: "running" }) })),
      request: request as unknown as BackendConnection["request"],
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
    request.mockClear();

    act(() => {
      vi.advanceTimersByTime(1200);
    });
    expect(postHostMessage).not.toHaveBeenCalledWith(expect.objectContaining({ type: "task.snapshot" }));
    expect(request).not.toHaveBeenCalled();
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
  handleNotification?: BackendConnection["handleNotification"];
  handleGenerationInvalidated?: BackendConnection["handleGenerationInvalidated"];
  handleRecoveryBaseline?: BackendConnection["handleRecoveryBaseline"];
  close: () => void;
};

function navigationBootstrap(options: { archived?: boolean; projectId?: string } = {}) {
  return {
    surface: "navigation" as const,
    shell: { kind: "vscodeExtension" as const, navigationMode: "currentProject" as const },
    archived: options.archived,
    projectId: options.projectId,
    agents: [],
    preferences: { composer_submit_shortcut: "mod_enter" as const },
  };
}

function taskBootstrap(taskId: string) {
  return {
    surface: "task" as const,
    shell: { kind: "vscodeExtension" as const, navigationMode: "currentProject" as const },
    taskId,
    agents: [],
    preferences: { composer_submit_shortcut: "mod_enter" as const },
  };
}

function settingsBootstrap() {
  return {
    surface: "settings" as const,
    shell: { kind: "vscodeExtension" as const, navigationMode: "currentProject" as const },
    agents: [],
    preferences: { composer_submit_shortcut: "mod_enter" as const },
  };
}

function webTaskBootstrap(taskId?: string, projectId?: string) {
  return {
    surface: "task" as const,
    shell: { kind: "web" as const, navigationMode: "project" as const },
    taskId,
    projectId,
    clientInstanceId: "client_1" as never,
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
    shell: { kind: "web" as const, navigationMode: "project" as const },
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
    lifecycle: "visible",
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
    history_sync: { state: "idle", generation: 0 },
    chat: {
      task_id: taskId,
      items: [],
      has_before: false,
      has_messages: true,
      total_count: 0,
      version: 1,
    },
    active_requests: [],
    send_capability: { state: "ready" },
    settings_summary: {
      agent_id: "codex",
      isolation: "local",
    },
    revision: 1,
  };
}

function clientSnapshot(
  options: {
    activeTaskTitle?: string;
    activeTaskStatus?: "idle" | "running";
    agents?: NonNullable<ClientSnapshot["agents"]>["agents"];
    newTaskDefaultAgentId?: string;
    includeTasks?: boolean;
    includeActiveTask?: boolean;
    appPreferences?: NonNullable<ClientSnapshot["settings"]>["preferences"];
    runtimeSettings?: NonNullable<ClientSnapshot["settings"]>["runtime"];
    stateRootId?: string;
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
    stateRoot: { stateRootId: (options.stateRootId ?? "state_root_1") as never },
    client: {
      clientInstanceId: "client_1" as never,
      shellKind: "vscodeExtension",
      surface: { kind: "home" },
    },
    newTaskDefaults: {
      agentId: (options.newTaskDefaultAgentId ?? "codex") as never,
    },
    agents: {
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
    title: { value: title, source: "user" as const },
    status,
    updatedAt: "2026-05-22T00:00:00.000Z",
    lastActivity: "2026-05-22T00:00:00.000Z",
    unread: false,
    hasMessages,
    workspaceAvailable: true,
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
): ProtocolTaskSnapshot {
  const status = typeof options === "string" ? options : options.status ?? "idle";
  const hasMessages = typeof options === "string" ? true : options.hasMessages ?? true;
  const sendReady = typeof options === "string" ? true : options.sendReady ?? true;
  const userText = typeof options === "string" ? undefined : options.userText;
  return {
    task: protocolTaskSummary(taskId, title, status, hasMessages),
    lifecycle: hasMessages ? "visible" : "new",
    revision: 1,
    preparation: { kind: "ready" as const },
    agentConfig: { state: "ready" as const, options: [] },
    agentCommands: { state: "ready" as const, commands: [] },
    sendCapability: sendReady
      ? { state: "ready" as const }
      : status === "running"
        ? {
            state: "blocked" as const,
            blockers: [{ kind: "taskRunning" as const, message: "Task is already running" }],
          }
        : { state: "loading" as const },
    historySync: { state: "idle", generation: 0 },
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

function taskSubscriptionSnapshot(
  cursor: string,
  task = protocolTaskSnapshot("task_1", "Task"),
) {
  const scope = { kind: "task" as const, taskId: "task_1" as never };
  return {
    cursor: cursor as never,
    scope,
    snapshot: {
      kind: "task" as const,
      task,
    },
  };
}

function nonTaskSubscriptionSnapshot(
  scope: { kind: string; taskId?: string } | undefined,
  cursor = "cursor_navigation",
) {
  if (scope?.kind === "projects") {
    return {
      cursor: cursor as never,
      scope,
      snapshot: {
        kind: "projects" as const,
        projects: { projects: [] },
      },
    };
  }
  if (scope?.kind === "agents") {
    return {
      cursor: cursor as never,
      scope,
      snapshot: {
        kind: "agents" as const,
        agents: {
          agents: [{ agentId: "codex" as never, label: "Codex", status: "connected" as const }],
        },
      },
    };
  }
  if (scope?.kind === "taskNavigation") {
    return {
      cursor: cursor as never,
      scope,
      snapshot: {
        kind: "taskNavigation" as const,
        navigation: { activeTaskId: "task_1" as never, tasks: [] },
      },
    };
  }
  throw new Error(`unexpected subscription scope: ${scope?.kind ?? "missing"}`);
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
