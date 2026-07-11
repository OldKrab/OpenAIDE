import { randomUUID } from "node:crypto";
import {
  createLocalHttpBackendConnection,
  type BackendConnection,
  type ClientInstanceId,
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

  constructor(private readonly provider: ConnectionProvider) {}

  async request<M extends ProtocolMethod>(
    method: M,
    params: RequestParamsByMethod[M],
    meta?: RequestMeta,
  ): Promise<ResponseResultByMethod[M]> {
    const connection = await this.ensureInitialized();
    return connection.request(method, params, meta);
  }

  dispose() {
    void this.connection?.close();
    this.connection = undefined;
    this.initialized = undefined;
  }

  private async ensureInitialized(): Promise<BackendConnection> {
    if (this.initialized) return this.initialized;

    this.initialized = (async () => {
      const info = await this.provider.startAppServerConnection();
      if (info.kind !== "localHttp") {
        throw new Error("VS Code host App Server client requires a local HTTP connection.");
      }
      const connection = createLocalHttpBackendConnection({
        ...info,
        connectionId: this.clientInstanceId,
      });
      await connection.initialize({
        clientInstanceId: this.clientInstanceId,
        shell: { kind: "vscodeExtension", name: "OpenAIDE" },
        requestedSurface: { kind: "home" },
        capabilities: { shell: ["resolveFileReveal"] },
      });
      this.connection = connection;
      return connection;
    })().catch((error) => {
      this.initialized = undefined;
      throw error;
    });

    return this.initialized;
  }
}
