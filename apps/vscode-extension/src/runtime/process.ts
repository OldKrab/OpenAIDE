import { ChildProcessWithoutNullStreams, spawn } from "node:child_process";
import * as path from "node:path";
import * as vscode from "vscode";
import type { WebviewAppServerConnection } from "@openaide/app-shell-contracts";
import { ExtensionLogger } from "../logging/logger";
import { resolveRuntimePath, type RuntimeSourceKind, type StorageRootKind } from "./paths";

export const RUNTIME_SHUTDOWN_GRACE_MS = 10_000;
const APP_SERVER_HANDOFF_TIMEOUT_MS = 5_000;
const APP_SERVER_HANDOFF_MAX_LINE_BYTES = 8 * 1024;
const APP_SERVER_HEALTH_INTERVAL_MS = 5_000;
const APP_SERVER_HEALTH_TIMEOUT_MS = 3_000;
const APP_SERVER_HEALTH_FAILURE_THRESHOLD = 2;
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
  private readonly appServerChildren = new Set<ChildProcessWithoutNullStreams>();
  private appServerConnection: Promise<WebviewAppServerConnection> | undefined;
  private currentAppServerConnection: WebviewAppServerConnection | undefined;
  private appServerRecovery: Promise<void> | undefined;
  private appServerHealthTimer: NodeJS.Timeout | undefined;
  private appServerHealthCheckInFlight = false;
  private appServerHealthFailures = 0;
  private disposed = false;
  private readonly exitListeners = new Set<(event: { code: number | null; signal: NodeJS.Signals | null }) => void>();
  private readonly appServerConnectionListeners = new Set<(connection: WebviewAppServerConnection) => void>();

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
    if (this.disposed) throw new Error("Runtime process is disposed");
    if (this.currentAppServerConnection) return this.currentAppServerConnection;
    if (this.appServerConnection) return this.appServerConnection;

    const launch = this.launchAppServerConnection()
      .then((connection) => {
        if (this.disposed) throw new Error("Runtime process was disposed during App Server handoff");
        this.currentAppServerConnection = connection;
        this.startAppServerHealthMonitor();
        return connection;
      })
      .finally(() => {
        if (this.appServerConnection === launch) this.appServerConnection = undefined;
      });
    this.appServerConnection = launch;
    return this.appServerConnection;
  }

  dispose() {
    this.disposed = true;
    if (this.appServerHealthTimer) clearInterval(this.appServerHealthTimer);
    this.appServerHealthTimer = undefined;
    if (this.child && !this.child.killed) {
      requestRuntimeShutdown(this.child);
    }
    for (const child of this.appServerChildren) {
      if (!child.killed) requestRuntimeShutdown(child);
    }
    this.appServerChildren.clear();
    this.child = undefined;
    this.appServerConnection = undefined;
    this.currentAppServerConnection = undefined;
    this.appServerConnectionListeners.clear();
  }

  onExit(listener: (event: { code: number | null; signal: NodeJS.Signals | null }) => void) {
    this.exitListeners.add(listener);
    return {
      dispose: () => this.exitListeners.delete(listener),
    };
  }

  /** Publishes only physical endpoint generations; the Frontend keeps one logical session. */
  onAppServerConnectionChanged(listener: (connection: WebviewAppServerConnection) => void) {
    this.appServerConnectionListeners.add(listener);
    return {
      dispose: () => this.appServerConnectionListeners.delete(listener),
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

    const environment = process.env.OPENAIDE_STORAGE_ROOT;
    if (environment) return { kind: "environment", path: environment };

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
    this.appServerChildren.add(child);
    child.stderr.on("data", (chunk: Buffer) => {
      this.logger.warn("app server handoff stderr", { byteLength: chunk.byteLength });
    });
    child.once("exit", (code, signal) => {
      this.logger.warn("app server handoff exited", { code, signal });
      this.appServerChildren.delete(child);
    });
    child.once("error", (error) => {
      this.logger.error("app server handoff spawn failed", { error: error.message });
      this.appServerChildren.delete(child);
    });

    try {
      return parseAppServerConnection(await readFirstStdoutLine(child));
    } catch (error) {
      if (!child.killed) child.kill();
      throw error;
    }
  }

  /** Health is shell-owned and intentionally independent from product-client heartbeats. */
  private startAppServerHealthMonitor() {
    if (this.appServerHealthTimer || this.disposed) return;
    this.appServerHealthTimer = setInterval(() => {
      void this.checkAppServerHealth();
    }, APP_SERVER_HEALTH_INTERVAL_MS);
    this.appServerHealthTimer.unref?.();
  }

  private async checkAppServerHealth() {
    const connection = this.currentAppServerConnection;
    if (
      !connection
      || connection.kind !== "localHttp"
      || this.disposed
      || this.appServerHealthCheckInFlight
      || this.appServerRecovery
    ) return;
    this.appServerHealthCheckInFlight = true;
    try {
      await probeAppServer(connection);
      this.appServerHealthFailures = 0;
    } catch (error) {
      this.appServerHealthFailures += 1;
      this.logger.warn("app server health probe failed", {
        consecutiveFailures: this.appServerHealthFailures,
        error: String(error),
      });
      if (this.appServerHealthFailures >= APP_SERVER_HEALTH_FAILURE_THRESHOLD) {
        await this.recoverAppServerConnection(connection);
      }
    } finally {
      this.appServerHealthCheckInFlight = false;
    }
  }

  private async recoverAppServerConnection(previous: WebviewAppServerConnection) {
    if (this.appServerRecovery || this.disposed) return this.appServerRecovery;
    this.logger.warn("app server endpoint unavailable; starting handoff recovery");
    const recovery = this.launchAppServerConnection()
      .then((replacement) => {
        if (this.disposed) return;
        this.currentAppServerConnection = replacement;
        this.appServerHealthFailures = 0;
        if (sameAppServerConnection(previous, replacement)) {
          this.logger.info("app server handoff recovered the existing endpoint");
          return;
        }
        this.logger.info("app server replacement endpoint ready");
        for (const listener of this.appServerConnectionListeners) listener(replacement);
      })
      .catch((error) => {
        this.logger.error("app server handoff recovery failed", { error: String(error) });
      })
      .finally(() => {
        if (this.appServerRecovery === recovery) this.appServerRecovery = undefined;
      });
    this.appServerRecovery = recovery;
    return recovery;
  }
}

async function probeAppServer(connection: Extract<WebviewAppServerConnection, { kind: "localHttp" }>) {
  const abort = new AbortController();
  const timeout = setTimeout(() => abort.abort(), APP_SERVER_HEALTH_TIMEOUT_MS);
  timeout.unref?.();
  try {
    const response = await fetch(connection.endpointUrl, {
      method: "POST",
      headers: {
        Authorization: `Bearer ${connection.authToken}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        jsonrpc: "2.0",
        id: "vscode-host-health",
        method: "client/probe",
        params: {},
      }),
      signal: abort.signal,
    });
    if (!response.ok) throw new Error(`health probe returned HTTP ${response.status}`);
  } finally {
    clearTimeout(timeout);
  }
}

function sameAppServerConnection(
  left: WebviewAppServerConnection,
  right: WebviewAppServerConnection,
) {
  return left.kind === right.kind
    && left.endpointUrl === right.endpointUrl
    && (left.kind !== "localHttp" || (right.kind === "localHttp" && left.authToken === right.authToken));
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
