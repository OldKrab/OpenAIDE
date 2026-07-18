import { ChildProcessWithoutNullStreams, spawn } from "node:child_process";
import * as path from "node:path";
import * as vscode from "vscode";
import type { WebviewAppServerConnection } from "@openaide/app-shell-contracts";
import { ExtensionLogger } from "../logging/logger";
import { resolveRuntimePath, type RuntimeSourceKind, type StorageRootKind } from "./paths";

export const RUNTIME_SHUTDOWN_GRACE_MS = 10_000;
const APP_SERVER_HANDOFF_TIMEOUT_MS = 5_000;
const APP_SERVER_HANDOFF_MAX_LINE_BYTES = 8 * 1024;
const SHELL_CONTROL_STDIO_PROTOCOL = "shell-control-stdio";

type RuntimeShutdownChild = Pick<ChildProcessWithoutNullStreams, "killed" | "kill" | "once" | "stdin">;

export function requestRuntimeShutdown(
  child: RuntimeShutdownChild,
  timeoutMs = RUNTIME_SHUTDOWN_GRACE_MS,
): NodeJS.Timeout | undefined {
  if (child.killed) return undefined;

  let timer: NodeJS.Timeout | undefined;
  const clearTimer = () => {
    if (timer) {
      clearTimeout(timer);
      timer = undefined;
    }
  };

  timer = setTimeout(() => {
    if (!child.killed) {
      child.kill();
    }
  }, timeoutMs);
  timer.unref?.();
  child.once("exit", clearTimer);
  child.once("error", clearTimer);

  try {
    if (child.stdin.destroyed || child.stdin.writableEnded) {
      clearTimer();
      child.kill();
      return undefined;
    }
    child.stdin.end(`${JSON.stringify({ jsonrpc: "2.0", id: "dispose", method: "runtime.shutdown", params: {} })}\n`);
  } catch {
    clearTimer();
    child.kill();
    return undefined;
  }

  return timer;
}

export class RuntimeProcess implements vscode.Disposable {
  private child: ChildProcessWithoutNullStreams | undefined;
  private appServerChild: ChildProcessWithoutNullStreams | undefined;
  private appServerConnection: Promise<WebviewAppServerConnection> | undefined;
  private readonly exitListeners = new Set<(event: { code: number | null; signal: NodeJS.Signals | null }) => void>();

  constructor(
    private readonly context: vscode.ExtensionContext,
    private readonly logger: ExtensionLogger,
  ) {
    this.logger.setLogFile(path.join(this.resolveStorageRoot().path, "diagnostics", "logs", "openaide-extension.jsonl"));
  }

  async start() {
    if (this.child && !this.child.killed) return this.child;

    const command = this.resolveRuntimePath();
    const storageRoot = this.resolveStorageRoot();
    this.logger.info("starting app server", { commandKind: command.kind, storageRootKind: storageRoot.kind });

    this.child = spawn(command.path, [], {
      env: {
        ...process.env,
        OPENAIDE_STORAGE_ROOT: storageRoot.path,
        OPENAIDE_APP_SERVER_PROTOCOL: SHELL_CONTROL_STDIO_PROTOCOL,
      },
      stdio: "pipe",
    });

    this.child.once("exit", (code, signal) => {
      this.logger.warn("app server exited", { code, signal });
      this.child = undefined;
      for (const listener of this.exitListeners) {
        listener({ code, signal });
      }
    });

    this.child.once("error", (error) => {
      this.logger.error("app server spawn failed", { error: error.message });
      this.child = undefined;
      for (const listener of this.exitListeners) {
        listener({ code: null, signal: null });
      }
    });

    this.child.stderr.on("data", (chunk: Buffer) => {
      this.logger.warn("app server stderr", { byteLength: chunk.byteLength });
    });

    return this.child;
  }

  async startAppServerConnection(): Promise<WebviewAppServerConnection> {
    if (this.appServerConnection) return this.appServerConnection;

    this.appServerConnection = this.launchAppServerConnection().catch((error) => {
      this.appServerConnection = undefined;
      throw error;
    });
    return this.appServerConnection;
  }

  dispose() {
    if (this.child && !this.child.killed) {
      requestRuntimeShutdown(this.child);
    }
    if (this.appServerChild && !this.appServerChild.killed) {
      requestRuntimeShutdown(this.appServerChild);
    }
    this.child = undefined;
    this.appServerChild = undefined;
    this.appServerConnection = undefined;
  }

  onExit(listener: (event: { code: number | null; signal: NodeJS.Signals | null }) => void) {
    this.exitListeners.add(listener);
    return {
      dispose: () => this.exitListeners.delete(listener),
    };
  }

  describe(): {
    running: boolean;
    runtime_source_kind: RuntimeSourceKind;
    storage_root_kind: StorageRootKind;
  } {
    const storageRoot = this.resolveStorageRoot();
    return {
      running: Boolean(this.child && !this.child.killed),
      runtime_source_kind: this.resolveRuntimePath().kind,
      storage_root_kind: storageRoot.kind,
    };
  }

  /** Returns shell-private inputs used to build a Support Export, never snapshot fields. */
  describeSupportHost() {
    return {
      diagnostics_log_directory: path.join(this.resolveStorageRoot().path, "diagnostics", "logs"),
      extension_version: safePackageVersion(this.context.extension.packageJSON.version),
    };
  }

  private resolveRuntimePath(): { kind: RuntimeSourceKind; path: string } {
    return resolveRuntimePath({
      extensionRoot: this.context.extensionUri.fsPath,
      configuredPath:
        vscode.workspace.getConfiguration("openaide").get<string>("appServer.path")
        ?? vscode.workspace.getConfiguration("openaide").get<string>("runtime.path"),
      envPath: process.env.OPENAIDE_APP_SERVER_PATH ?? process.env.OPENAIDE_RUNTIME_PATH,
    });
  }

  private resolveStorageRoot(): { kind: StorageRootKind; path: string } {
    const configured = vscode.workspace.getConfiguration("openaide").get<string>("storage.root");
    if (configured) return { kind: "configured", path: configured };

    return {
      kind: "extension-storage",
      path: path.join(this.context.globalStorageUri.fsPath, "runtime"),
    };
  }

  private async launchAppServerConnection(): Promise<WebviewAppServerConnection> {
    const command = this.resolveRuntimePath();
    const storageRoot = this.resolveStorageRoot();
    this.logger.info("starting app server handoff", {
      commandKind: command.kind,
      storageRootKind: storageRoot.kind,
    });
    const child = spawn(command.path, [], {
      env: {
        ...process.env,
        OPENAIDE_STORAGE_ROOT: storageRoot.path,
        OPENAIDE_APP_SERVER_PROTOCOL: "app-server-handoff",
      },
      stdio: "pipe",
    });
    this.appServerChild = child;
    child.stderr.on("data", (chunk: Buffer) => {
      this.logger.warn("app server handoff stderr", { byteLength: chunk.byteLength });
    });
    child.once("exit", (code, signal) => {
      this.logger.warn("app server handoff exited", { code, signal });
      if (this.appServerChild === child) {
        this.appServerChild = undefined;
        this.appServerConnection = undefined;
      }
    });
    child.once("error", (error) => {
      this.logger.error("app server handoff spawn failed", { error: error.message });
      if (this.appServerChild === child) {
        this.appServerChild = undefined;
        this.appServerConnection = undefined;
      }
    });

    try {
      return parseAppServerConnection(await readFirstStdoutLine(child));
    } catch (error) {
      if (!child.killed) child.kill();
      throw error;
    }
  }
}

function safePackageVersion(value: unknown) {
  return typeof value === "string" ? value : "unknown";
}

function readFirstStdoutLine(child: ChildProcessWithoutNullStreams): Promise<string> {
  return new Promise((resolve, reject) => {
    let buffer = "";
    const timer = setTimeout(() => {
      cleanup();
      reject(new Error("OpenAIDE App Server handoff timed out before connection info"));
    }, APP_SERVER_HANDOFF_TIMEOUT_MS);
    timer.unref?.();
    const cleanup = () => {
      clearTimeout(timer);
      child.stdout.off("data", onData);
      child.off("exit", onExit);
      child.off("error", onError);
    };
    const onData = (chunk: Buffer) => {
      buffer += chunk.toString("utf8");
      if (Buffer.byteLength(buffer, "utf8") > APP_SERVER_HANDOFF_MAX_LINE_BYTES) {
        cleanup();
        reject(new Error("OpenAIDE App Server handoff connection info is too large"));
        return;
      }
      const newline = buffer.indexOf("\n");
      if (newline < 0) return;
      const line = buffer.slice(0, newline).trim();
      cleanup();
      resolve(line);
    };
    const onExit = () => {
      cleanup();
      reject(new Error("OpenAIDE App Server handoff exited before connection info"));
    };
    const onError = (error: Error) => {
      cleanup();
      reject(error);
    };
    child.stdout.on("data", onData);
    child.once("exit", onExit);
    child.once("error", onError);
  });
}

function parseAppServerConnection(line: string): WebviewAppServerConnection {
  const parsed = JSON.parse(line) as unknown;
  if (!parsed || typeof parsed !== "object") {
    throw new Error("OpenAIDE App Server handoff returned invalid connection info");
  }
  const record = parsed as Record<string, unknown>;
  if (
    record.kind !== "localHttp" ||
    typeof record.endpointUrl !== "string" ||
    typeof record.authToken !== "string" ||
    record.authToken.length === 0 ||
    !isLoopbackLocalHttpEndpoint(record.endpointUrl)
  ) {
    throw new Error("OpenAIDE App Server handoff returned invalid LocalHttp connection info");
  }
  return {
    kind: "localHttp",
    endpointUrl: record.endpointUrl,
    authToken: record.authToken,
  };
}

function isLoopbackLocalHttpEndpoint(value: string) {
  try {
    const url = new URL(value);
    return url.protocol === "http:" &&
      url.port.length > 0 &&
      url.pathname === "/probe" &&
      (url.hostname === "127.0.0.1" || url.hostname === "localhost" || url.hostname === "[::1]");
  } catch {
    return false;
  }
}
