import { beforeEach, describe, expect, it, vi } from "vitest";
import * as vscode from "vscode";
import {
  ATTACHMENT_CREATE_LOCAL_FILE_REFERENCES,
  SETTINGS_GET_RUNTIME,
} from "@openaide/app-server-client";
import { handleWebviewMessage } from "./messaging";

const workspaceMocks = vi.hoisted(() => ({
  firstWorkspaceRoot: vi.fn(),
  workspaceRoots: vi.fn(),
}));
const settingsMocks = vi.hoisted(() => ({
  unlockDeveloperSettings: vi.fn(async (store: { update: (key: string, value: boolean) => PromiseLike<void> | void }) => {
    await store.update("openaide.developerSettingsUnlocked", true);
  }),
}));
const diagnosticsMocks = vi.hoisted(() => ({
  collectDiagnostics: vi.fn(),
  exportSupportDiagnostics: vi.fn(),
}));
const fsMocks = vi.hoisted(() => ({
  realpath: vi.fn(async (path: string) => path),
}));

vi.mock("../workspace/roots", () => ({
  firstWorkspaceRoot: workspaceMocks.firstWorkspaceRoot,
  workspaceRoots: workspaceMocks.workspaceRoots,
}));

vi.mock("../settings/snapshot", () => ({
  unlockDeveloperSettings: settingsMocks.unlockDeveloperSettings,
}));

vi.mock("../diagnostics/snapshot", () => ({
  collectDiagnostics: diagnosticsMocks.collectDiagnostics,
}));

vi.mock("../diagnostics/export", () => ({
  exportSupportDiagnostics: diagnosticsMocks.exportSupportDiagnostics,
}));

vi.mock("node:fs/promises", () => ({
  realpath: fsMocks.realpath,
}));

vi.mock("vscode", () => ({
  commands: {
    executeCommand: vi.fn(),
  },
  env: { openExternal: vi.fn() },
  Range: class {
    endCharacter: number;
    endLine: number;
    startCharacter: number;
    startLine: number;

    constructor(startLine: number, startCharacter: number, endLine: number, endCharacter: number) {
      this.endCharacter = endCharacter;
      this.endLine = endLine;
      this.startCharacter = startCharacter;
      this.startLine = startLine;
    }
  },
  Uri: {
    file: (path: string) => ({ fsPath: path }),
    joinPath: (...parts: Array<{ fsPath?: string } | string>) => ({
      fsPath: parts.map((part) => (typeof part === "string" ? part : (part.fsPath ?? ""))).join("/"),
    }),
    parse: (value: string) => ({ value }),
  },
  window: {
    showErrorMessage: vi.fn(),
    showInformationMessage: vi.fn(),
    showOpenDialog: vi.fn(),
    showTextDocument: vi.fn(),
    showWarningMessage: vi.fn(),
  },
  workspace: {
    openTextDocument: vi.fn(),
    workspaceFolders: [{ uri: { fsPath: "/workspace/app" } }],
  },
}));

describe("webview messaging composer routes", () => {
  beforeEach(() => {
    vi.mocked(vscode.commands.executeCommand).mockClear();
    vi.mocked(vscode.env.openExternal).mockClear();
    vi.mocked(vscode.workspace.openTextDocument).mockClear();
    vi.mocked(vscode.window.showTextDocument).mockClear();
    vi.mocked(vscode.window.showInformationMessage).mockClear();
    vi.mocked(vscode.window.showOpenDialog).mockReset();
    vi.mocked(vscode.window.showWarningMessage).mockClear();
    vi.mocked(vscode.window.showErrorMessage).mockClear();
    workspaceMocks.firstWorkspaceRoot.mockReturnValue("/workspace/fallback");
    workspaceMocks.workspaceRoots.mockReturnValue([{ path: "/workspace/app", label: "App" }]);
    diagnosticsMocks.collectDiagnostics.mockReset();
    diagnosticsMocks.collectDiagnostics.mockResolvedValue({
      created_at: "2026-05-18T00:00:00.000Z",
      runtime: {
        status: "ready",
        method_count: 4,
        tasks: { visible_count: 1, total_count: 2, active_count: 0, revision: 7 },
        redaction: "prompt_text_file_contents_terminal_output_and_secrets_removed",
      },
      notices: [],
      process: { running: true },
    });
    diagnosticsMocks.exportSupportDiagnostics.mockReset();
  });

  it("logs webview action lifecycle with safe diagnostic fields", async () => {
    const runtime = {};
    const posted: unknown[] = [];
    const logger = { warn: vi.fn(), info: vi.fn() };

    await handleWebviewMessage(
      {
        type: "surface.openTask",
        payload: { task_id: "task_1", title: "Fix ACP" },
      },
      context(runtime, posted, { openTask: vi.fn() }, logger),
    );

    expect(logger.info).toHaveBeenNthCalledWith(
      1,
      "webview action received",
      expect.objectContaining({
        type: "surface.openTask",
      }),
    );
    expect(logger.info).toHaveBeenNthCalledWith(
      2,
      "webview action completed",
      expect.objectContaining({ type: "surface.openTask" }),
    );
    expect(JSON.stringify(logger.info.mock.calls)).not.toContain("Fix ACP");
  });

  it("logs safe webview telemetry fields", async () => {
    const posted: unknown[] = [];
    const logger = { warn: vi.fn(), info: vi.fn() };

    await handleWebviewMessage(
      {
        type: "webview.telemetry",
        payload: {
          event: "render_error",
          surface: "task",
          reason: "unknown_request",
          task_id: "task_1",
          snapshot_request_id: 3,
          snapshot_intent: "open",
          has_active_task: true,
          error_name: "TypeError",
          error_message: "Cannot read /workspace/private",
          path: "/workspace/private",
        },
      },
      context({}, posted, undefined, logger),
    );

    expect(logger.info).toHaveBeenCalledWith(
      "webview telemetry",
      expect.objectContaining({
        event: "render_error",
        surface: "task",
        reason: "unknown_request",
        task_id: "task_1",
        snapshot_request_id: 3,
        snapshot_intent: "open",
        has_active_task: true,
        error_name: "TypeError",
        error_message: "Cannot read [path]",
      }),
    );
    expect(JSON.stringify(logger.info.mock.calls)).not.toContain("/workspace/private");
    expect(posted).toEqual([]);
  });

  it("records native session load failures as structured error logs", async () => {
    const posted: unknown[] = [];
    const logger = { warn: vi.fn(), info: vi.fn(), error: vi.fn() };

    await handleWebviewMessage(
      {
        type: "webview.telemetry",
        payload: {
          event: "native_sessions_load_failed",
          surface: "navigation",
          request: "agent/listSessions",
          session_list_request_id: 4,
          agent_id: "codex",
          project_id: "project-current",
          error_name: "AppServerProtocolError",
          error_code: "notFound",
          error_message: "Project /workspace/private was not found",
        },
      },
      context({}, posted, undefined, logger),
    );

    expect(logger.error).toHaveBeenCalledWith("webview telemetry", {
      event: "native_sessions_load_failed",
      surface: "navigation",
      request: "agent/listSessions",
      session_list_request_id: 4,
      agent_id: "codex",
      project_id: "project-current",
      error_name: "AppServerProtocolError",
      error_code: "notFound",
      error_message: "Project [path] was not found",
    });
    expect(posted).toEqual([]);
  });

  it("answers App Server secret read requests from VS Code secrets", async () => {
    const posted: unknown[] = [];
    const agentSecretStore = {
      get: vi.fn(async (key: string) => key === "agent.secret" ? "secret-value" : undefined),
    };

    await handleWebviewMessage(
      {
        type: "appServer.serverRequest",
        payload: {
          requestId: "server-request-1",
          method: "secret/read",
          params: { key: "agent.secret", label: "Agent secret" },
        },
      },
      context({}, posted, undefined, undefined, undefined, { agentSecretStore }),
    );

    expect(agentSecretStore.get).toHaveBeenCalledWith("agent.secret");
    expect(posted).toEqual([
      {
        type: "appServer.serverRequest.result",
        payload: {
          requestId: "server-request-1",
          method: "secret/read",
          result: { value: "secret-value" },
        },
      },
    ]);
  });

  it("stores, migrates, and deletes Agent secrets without echoing values", async () => {
    const posted: unknown[] = [];
    const logger = { warn: vi.fn(), info: vi.fn() };
    const agentSecretStore = {
      get: vi.fn(async (key: string) => key.endsWith("custom.old.env.EXISTING") ? "preserved-secret" : undefined),
      store: vi.fn(async () => undefined),
      delete: vi.fn(async () => undefined),
    };

    await handleWebviewMessage(
      {
        type: "secret.transaction.apply",
        payload: {
          requestId: "secret-request-1",
          transactionId: "secret-transaction-1",
          changes: {
            writes: [
            {
              target: { kind: "agentEnvironment", agentId: "custom.new", name: "TOKEN" },
              value: "rotated-secret",
            },
            {
              target: { kind: "agentEnvironment", agentId: "custom.new", name: "EXISTING" },
              copyFrom: { kind: "agentEnvironment", agentId: "custom.old", name: "EXISTING" },
            },
          ],
            deletes: [
              { kind: "agentEnvironment", agentId: "custom.old", name: "TOKEN" },
              { kind: "agentEnvironment", agentId: "custom.old", name: "EXISTING" },
            ],
          },
        },
      },
      context({}, posted, undefined, logger, undefined, { agentSecretStore }),
    );

    expect(agentSecretStore.get).toHaveBeenCalledWith("openaide.agent.custom.old.env.EXISTING");
    expect(agentSecretStore.store.mock.calls).toEqual([
      ["openaide.agent.custom.new.env.TOKEN", "rotated-secret"],
      ["openaide.agent.custom.new.env.EXISTING", "preserved-secret"],
    ]);
    expect(agentSecretStore.delete.mock.calls).toEqual([
      ["openaide.agent.custom.old.env.TOKEN"],
      ["openaide.agent.custom.old.env.EXISTING"],
    ]);
    expect(posted).toEqual([{
      type: "secret.transaction.result",
      payload: {
        requestId: "secret-request-1",
        transactionId: "secret-transaction-1",
        ok: true,
      },
    }]);
    expect(JSON.stringify(logger.info.mock.calls)).not.toContain("rotated-secret");
  });

  it("reports unavailable secret storage without returning the submitted value", async () => {
    const posted: unknown[] = [];

    await handleWebviewMessage(
      {
        type: "secret.transaction.apply",
        payload: {
          requestId: "secret-request-2",
          transactionId: "secret-transaction-2",
          changes: {
            writes: [{
              target: { kind: "agentEnvironment", agentId: "custom.new", name: "TOKEN" },
              value: "never-echo-this",
            }],
            deletes: [],
          },
        },
      },
      context({}, posted),
    );

    expect(posted).toEqual([
      {
        type: "secret.transaction.result",
        payload: {
          requestId: "secret-request-2",
          transactionId: "secret-transaction-2",
          ok: false,
          error: "Secure storage is unavailable.",
        },
      },
    ]);
    expect(JSON.stringify(posted)).not.toContain("never-echo-this");
  });

  it("answers App Server shell notification requests with selected action id", async () => {
    const posted: unknown[] = [];
    vi.mocked(vscode.window.showWarningMessage).mockResolvedValue("Open Settings" as never);

    await handleWebviewMessage(
      {
        type: "appServer.serverRequest",
        payload: {
          requestId: "server-request-2",
          method: "shell/showNotification",
          params: {
            level: "warning",
            message: "Credential required",
            actions: [{ actionId: "open-settings", label: "Open Settings" }],
          },
        },
      },
      context({}, posted),
    );

    expect(vscode.window.showWarningMessage).toHaveBeenCalledWith(
      "Credential required",
      "Open Settings",
    );
    expect(posted).toEqual([
      {
        type: "appServer.serverRequest.result",
        payload: {
          requestId: "server-request-2",
          method: "shell/showNotification",
          result: { actionId: "open-settings" },
        },
      },
    ]);
  });

  it("answers opaque App Server reveal file requests without raw path fallback", async () => {
    const posted: unknown[] = [];

    await handleWebviewMessage(
      {
        type: "appServer.serverRequest",
        payload: {
          requestId: "server-request-3",
          method: "shell/revealFile",
          params: {
            originatingClientInstanceId: "client-1",
            fileHandleId: "file-handle-1",
            label: "main.rs",
          },
        },
      },
      context({}, posted),
    );

    expect(vscode.workspace.openTextDocument).not.toHaveBeenCalled();
    expect(posted).toEqual([
      {
        type: "appServer.serverRequest.result",
        payload: {
          requestId: "server-request-3",
          method: "shell/revealFile",
          result: { revealed: false },
        },
      },
    ]);
  });

  it("reveals App Server files in the VS Code Explorer when they belong to the workspace", async () => {
    const posted: unknown[] = [];
    const runtime = {
      appServerRequest: vi.fn(async () => ({
        path: "/workspace/app/src/main.rs",
        label: "main.rs",
      })),
    };

    await handleWebviewMessage(
      {
        type: "appServer.serverRequest",
        payload: {
          requestId: "server-request-4",
          method: "shell/revealFile",
          params: {
            originatingClientInstanceId: "client-1",
            fileHandleId: "file-reveal-1",
            label: "main.rs",
          },
        },
      },
      context(runtime, posted),
    );

    expect(runtime.appServerRequest).toHaveBeenCalledWith("shell/resolveFileReveal", {
      originatingClientInstanceId: "client-1",
      fileHandleId: "file-reveal-1",
    });
    expect(vscode.commands.executeCommand).toHaveBeenCalledWith(
      "revealInExplorer",
      { fsPath: "/workspace/app/src/main.rs" },
    );
    expect(vscode.workspace.openTextDocument).not.toHaveBeenCalled();
    expect(posted).toEqual([
      {
        type: "appServer.serverRequest.result",
        payload: {
          requestId: "server-request-4",
          method: "shell/revealFile",
          result: { revealed: true },
        },
      },
    ]);
  });

  it("reveals App Server files outside the workspace in the OS file manager", async () => {
    const posted: unknown[] = [];
    const runtime = {
      appServerRequest: vi.fn(async () => ({
        path: "/tmp/report.zip",
        label: "report.zip",
      })),
    };

    await handleWebviewMessage(
      {
        type: "appServer.serverRequest",
        payload: {
          requestId: "server-request-5",
          method: "shell/revealFile",
          params: {
            originatingClientInstanceId: "client-1",
            fileHandleId: "file-reveal-2",
            label: "report.zip",
          },
        },
      },
      context(runtime, posted),
    );

    expect(vscode.commands.executeCommand).toHaveBeenCalledWith(
      "revealFileInOS",
      { fsPath: "/tmp/report.zip" },
    );
    expect(posted).toContainEqual({
      type: "appServer.serverRequest.result",
      payload: {
        requestId: "server-request-5",
        method: "shell/revealFile",
        result: { revealed: true },
      },
    });
  });

  it("routes diagnostics snapshot and export requests", async () => {
    const runtime = { diagnostics: vi.fn() };
    const posted: unknown[] = [];
    const runtimeProcess = { dispose: vi.fn(), describe: vi.fn(() => ({ running: true })) };

    await handleWebviewMessage({ type: "diagnostics.snapshot" }, context(runtime, posted, undefined, undefined, undefined, { runtimeProcess }));
    await handleWebviewMessage({ type: "diagnostics.export" }, context(runtime, posted, undefined, undefined, undefined, { runtimeProcess }));

    expect(diagnosticsMocks.collectDiagnostics).toHaveBeenCalledWith(runtime, runtimeProcess);
    expect(diagnosticsMocks.exportSupportDiagnostics).toHaveBeenCalledWith(runtime, runtimeProcess);
    expect(posted[0]).toMatchObject({ type: "diagnostics.snapshot.result", payload: { runtime: { status: "ready" } } });
  });

  it("opens expanded tool paths inside the workspace", async () => {
    const posted: unknown[] = [];

    await handleWebviewMessage(
      { type: "tool.openPath", payload: { path: "/workspace/app/src/main.rs", line: 12 } },
      context({}, posted),
    );

    expect(vscode.workspace.openTextDocument).toHaveBeenCalledWith({ fsPath: "/workspace/app/src/main.rs" });
    expect(vscode.window.showTextDocument).toHaveBeenCalledWith(undefined, {
      preview: true,
      selection: { endCharacter: 0, endLine: 11, startCharacter: 0, startLine: 11 },
    });
    expect(posted).toEqual([]);
  });

  it("rejects expanded tool paths outside the workspace", async () => {
    const posted: unknown[] = [];

    await handleWebviewMessage(
      { type: "tool.openPath", payload: { path: "/tmp/secret.txt" } },
      context({}, posted),
    );

    expect(vscode.workspace.openTextDocument).not.toHaveBeenCalled();
    expect(posted[0]).toMatchObject({
      type: "runtime.error",
      payload: { action: "tool.openPath" },
    });
  });

  it("resolves worktree folders through the App Server before revealing them", async () => {
    const runtime = {
      appServerRequest: vi.fn().mockResolvedValue({ path: "/workspace/app/.worktrees/sidebar" }),
    };
    const posted: unknown[] = [];

    await handleWebviewMessage(
      {
        type: "worktree.openFolder",
        payload: { repository_id: "repository-1", worktree_id: "worktree-1" },
      },
      context(runtime, posted),
    );

    expect(runtime.appServerRequest).toHaveBeenCalledWith("worktree/resolveFolder", {
      repositoryId: "repository-1",
      worktreeId: "worktree-1",
    });
    expect(vscode.commands.executeCommand).toHaveBeenCalledWith(
      "revealInExplorer",
      { fsPath: "/workspace/app/.worktrees/sidebar" },
    );
    expect(posted).toEqual([]);
  });


  it("returns workspace roots to the webview", async () => {
    const posted: unknown[] = [];

    await handleWebviewMessage({ type: "workspace.roots" }, context({}, posted));

    expect(posted[0]).toEqual({
      type: "workspace.roots.result",
      payload: { roots: [{ path: "/workspace/app", label: "App" }] },
    });
  });

  it("persists developer settings unlock and returns refreshed runtime settings", async () => {
    const runtime = {
      appServerRequest: vi.fn().mockResolvedValue({
        developer: { acpTrace: { enabled: false, directory: "/runtime/diagnostics/acp-traces" } },
      }),
    };
    const posted: unknown[] = [];
    const developerSettingsStore = {
      get: vi.fn(() => false),
      update: vi.fn(async () => undefined),
    };

    await handleWebviewMessage(
      { type: "developer.settings.unlock" },
      context(runtime, posted, undefined, undefined, developerSettingsStore),
    );

    expect(developerSettingsStore.update).toHaveBeenCalledWith("openaide.developerSettingsUnlocked", true);
    expect(runtime.appServerRequest).toHaveBeenCalledWith(SETTINGS_GET_RUNTIME, {});
    expect(posted[0]).toEqual({
      type: "runtime.settings.result",
      payload: {
        developer: { acp_trace: { enabled: false, directory: "/runtime/diagnostics/acp-traces" } },
      },
    });
  });

  it("routes surface commands to editor managers", async () => {
    const posted: unknown[] = [];
    const calls: string[] = [];
    const surfaces = {
      openNewTask: vi.fn(),
      openSettings: vi.fn(),
      openTask: vi.fn(() => calls.push("open")),
    };
    const adoptTask = vi.fn(() => calls.push("adopt"));

    await handleWebviewMessage({ type: "surface.openNewTask" }, context({}, posted, surfaces));
    await handleWebviewMessage(
      { type: "surface.openNewTask", payload: { project_id: "project_1" } },
      context({}, posted, surfaces),
    );
    await handleWebviewMessage({ type: "surface.openSettings" }, context({}, posted, surfaces));
    await handleWebviewMessage(
      { type: "surface.openTask", payload: { task_id: "task_1", title: "Fix ACP" } },
      context({}, posted, surfaces, undefined, undefined, { adoptTask }),
    );

    expect(surfaces.openNewTask).toHaveBeenCalledTimes(2);
    expect(surfaces.openNewTask).toHaveBeenCalledWith("project_1");
    expect(surfaces.openSettings).toHaveBeenCalledTimes(1);
    expect(adoptTask).toHaveBeenCalledWith("task_1", "Fix ACP");
    expect(surfaces.openTask).toHaveBeenCalledWith("task_1", "Fix ACP");
    expect(calls).toEqual(["adopt", "open"]);
    expect(posted).toEqual([]);
  });

  it("routes explicit recovery capabilities through VS Code", async () => {
    const posted: unknown[] = [];
    await handleWebviewMessage(
      { type: "shell.openExternal", payload: { url: "https://nodejs.org/en/download" } },
      context({}, posted),
    );
    await handleWebviewMessage({ type: "shell.reload" }, context({}, posted));

    expect(vscode.env.openExternal).toHaveBeenCalledWith({ value: "https://nodejs.org/en/download" });
    expect(vscode.commands.executeCommand).toHaveBeenCalledWith("workbench.action.reloadWindow");
  });

  it("brokers native file paths directly from VS Code to the App Server", async () => {
    vi.mocked(vscode.window.showOpenDialog).mockResolvedValue([
      { fsPath: "/outside/large-model.bin" } as vscode.Uri,
    ]);
    const runtime = {
      appServerRequest: vi.fn().mockResolvedValue({
        attachments: [{ handleId: "attachment-1", label: "large-model.bin" }],
      }),
    };
    const posted: unknown[] = [];

    await handleWebviewMessage({
      type: "attachment.pickFiles",
      payload: { requestId: "pick-1", taskId: "task-1" },
    }, context(runtime, posted));

    expect(runtime.appServerRequest).toHaveBeenCalledWith(
      ATTACHMENT_CREATE_LOCAL_FILE_REFERENCES,
      { taskId: "task-1", paths: ["/outside/large-model.bin"] },
    );
    expect(posted).toEqual([{
      type: "attachment.pickFiles.result",
      payload: {
        requestId: "pick-1",
        attachments: [{ handleId: "attachment-1", label: "large-model.bin" }],
      },
    }]);
  });
});

function context(
  runtime: unknown,
  posted: unknown[],
  surfaces?: unknown,
  logger: { warn: ReturnType<typeof vi.fn>; info: ReturnType<typeof vi.fn> } = { warn: vi.fn(), info: vi.fn() },
  developerSettingsStore: unknown = { get: vi.fn(() => false), update: vi.fn(async () => undefined) },
  extras: Record<string, unknown> = {},
) {
  return {
    runtime,
    runtimeProcess: { dispose: vi.fn() },
    post: async (payload: unknown) => {
      posted.push(payload);
      return true;
    },
    logger,
    developerSettingsStore,
    surfaces,
    ...extras,
  } as never;
}
