import { randomUUID } from "node:crypto";
import {
  CLIENT_CAPABILITIES_CHANGED,
  createReliableLocalHttpBackendConnection,
  type BackendConnection,
  type ClientInstanceId,
  type ClientWorkspaceRoot,
  type ProtocolMethod,
  type RequestMeta,
  type RequestParamsByMethod,
  type ResponseResultByMethod,
} from "@openaide/app-server-client";
import type { WebviewAppServerConnection } from "@openaide/app-shell-contracts";

type ConnectionProvider = {
  startAppServerConnection(): Promise<WebviewAppServerConnection>;
};

export class AppServerHostClient {
  private readonly clientInstanceId = `vscode-host-${randomUUID()}` as ClientInstanceId;
  private connection: BackendConnection | undefined;
  private initialized: Promise<BackendConnection> | undefined;
  private desiredWorkspaceRoots: ClientWorkspaceRoot[] = [];
  private reportedWorkspaceRoots: ClientWorkspaceRoot[] | undefined;
  private workspaceSync: Promise<void> | undefined;

  constructor(private readonly provider: ConnectionProvider) {}

  async request<M extends ProtocolMethod>(
    method: M,
    params: RequestParamsByMethod[M],
    meta?: RequestMeta,
  ): Promise<ResponseResultByMethod[M]> {
    await this.flushWorkspaceRoots();
    const connection = await this.ensureInitialized();
    return connection.request(method, params, meta);
  }

  async syncWorkspaceRoots(roots: ClientWorkspaceRoot[]): Promise<void> {
    this.desiredWorkspaceRoots = normalizedWorkspaceRoots(roots);
    await this.flushWorkspaceRoots();
  }

  dispose() {
    void this.connection?.close();
    this.connection = undefined;
    this.initialized = undefined;
    this.reportedWorkspaceRoots = undefined;
    this.workspaceSync = undefined;
  }

  private async ensureInitialized(): Promise<BackendConnection> {
    if (this.initialized) return this.initialized;

    this.initialized = (async () => {
      const info = await this.provider.startAppServerConnection();
      if (info.kind !== "localHttp") {
        throw new Error("VS Code host App Server client requires a local HTTP connection.");
      }
      const connection = createReliableLocalHttpBackendConnection({
        ...info,
        connectionId: `vscode-connection-${randomUUID()}`,
      });
      const initializedWorkspaceRoots = cloneWorkspaceRoots(this.desiredWorkspaceRoots);
      await connection.initialize({
        clientInstanceId: this.clientInstanceId,
        shell: { kind: "vscodeExtension", name: "OpenAIDE" },
        requestedSurface: { kind: "home" },
        capabilities: { shell: ["resolveFileReveal"] },
        workspaceRoots: initializedWorkspaceRoots,
      });
      this.reportedWorkspaceRoots = initializedWorkspaceRoots;
      this.connection = connection;
      return connection;
    })().catch((error) => {
      this.initialized = undefined;
      throw error;
    });

    return this.initialized;
  }

  private async flushWorkspaceRoots(): Promise<void> {
    if (this.workspaceSync) {
      await this.workspaceSync;
      if (!sameWorkspaceRoots(this.reportedWorkspaceRoots, this.desiredWorkspaceRoots)) {
        await this.flushWorkspaceRoots();
      }
      return;
    }

    const sync = (async () => {
      const connection = await this.ensureInitialized();
      while (!sameWorkspaceRoots(this.reportedWorkspaceRoots, this.desiredWorkspaceRoots)) {
        const replacement = cloneWorkspaceRoots(this.desiredWorkspaceRoots);
        await connection.request(CLIENT_CAPABILITIES_CHANGED, { workspaceRoots: replacement });
        this.reportedWorkspaceRoots = replacement;
      }
    })();
    this.workspaceSync = sync;
    try {
      await sync;
    } finally {
      if (this.workspaceSync === sync) this.workspaceSync = undefined;
    }
  }
}

function normalizedWorkspaceRoots(roots: ClientWorkspaceRoot[]) {
  const paths = [...new Set(roots.map(({ path }) => path).filter((path) => path.trim()))].sort();
  return paths.map((path) => ({ path }));
}

function cloneWorkspaceRoots(roots: ClientWorkspaceRoot[]) {
  return roots.map(({ path }) => ({ path }));
}

function sameWorkspaceRoots(
  left: ClientWorkspaceRoot[] | undefined,
  right: ClientWorkspaceRoot[],
) {
  return left !== undefined
    && left.length === right.length
    && left.every((root, index) => root.path === right[index]?.path);
}
