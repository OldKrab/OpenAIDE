import { randomUUID } from "node:crypto";
import {
  CLIENT_CAPABILITIES_CHANGED,
  createReliableLocalHttpBackendConnection,
  isAppServerSessionViewMessage,
  serializeBridgeError,
  serializeSessionStatus,
  type AppServerSession,
  type AppServerSessionHostMessage,
  type AppServerSessionStatus,
  type AppServerSessionViewMessage,
  type BackendRequestContext,
  type BackendUnsubscribe,
  type ClientInstanceId,
  type ClientWorkspaceRoot,
  type InitializeResult,
  type ProtocolMethod,
  type RequestMeta,
  type RequestParamsByMethod,
  type ResponseResultByMethod,
  type ServerRequestMethod,
  type ServerRequestResponseResultByMethod,
} from "@openaide/app-server-client";
import type { WebviewAppServerConnection } from "@openaide/app-shell-contracts";

type ConnectionProvider = {
  startAppServerConnection(): Promise<WebviewAppServerConnection>;
};

type HostClientLogger = {
  info(message: string, fields?: Record<string, unknown>): void;
  warn(message: string, fields?: Record<string, unknown>): void;
};

type ViewState = {
  post(message: AppServerSessionHostMessage): void;
  subscriptions: Map<string, BackendUnsubscribe>;
  requestMethods: Set<ServerRequestMethod>;
};

type PendingServerRequest = {
  viewId: string;
  resolve(result: unknown): void;
  reject(error: unknown): void;
};

export class AppServerHostClient {
  private readonly clientInstanceId = `vscode-host-${randomUUID()}` as ClientInstanceId;
  private connection: AppServerSession | undefined;
  private initialized: Promise<AppServerSession> | undefined;
  private initializationResult: InitializeResult | undefined;
  private latestSessionStatus: AppServerSessionStatus | undefined;
  private desiredWorkspaceRoots: ClientWorkspaceRoot[] = [];
  private reportedWorkspaceRoots: ClientWorkspaceRoot[] | undefined;
  private workspaceSync: Promise<void> | undefined;
  private readonly views = new Map<string, ViewState>();
  private readonly sessionStops: BackendUnsubscribe[] = [];
  private readonly serverRequestStops = new Map<ServerRequestMethod, BackendUnsubscribe>();
  private readonly pendingServerRequests = new Map<string, PendingServerRequest>();
  private nextServerRequestId = 1;

  constructor(
    private readonly provider: ConnectionProvider,
    private readonly logger?: HostClientLogger,
  ) {}

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

  /** Registers one render surface behind the extension host's single logical client. */
  attachView(viewId: string, post: (message: AppServerSessionHostMessage) => void) {
    this.detachView(viewId);
    this.views.set(viewId, {
      post,
      subscriptions: new Map(),
      requestMethods: new Set(),
    });
    this.logger?.info("app server view attached to host client", { viewCount: this.views.size });
    return () => this.detachView(viewId);
  }

  async handleViewMessage(viewId: string, message: unknown): Promise<boolean> {
    if (!isAppServerSessionViewMessage(message)) return false;
    const view = this.views.get(viewId);
    if (!view) return true;

    if (message.type === "appServer.session.detach") {
      this.detachView(viewId);
      return true;
    }
    if (message.type === "appServer.session.serverResponse") {
      this.resolveServerRequest(viewId, message);
      return true;
    }
    if (message.type === "appServer.session.unsubscribe") {
      view.subscriptions.get(message.subscriptionId)?.();
      view.subscriptions.delete(message.subscriptionId);
      return true;
    }
    if (message.type === "appServer.session.unregisterRequestHandler") {
      view.requestMethods.delete(message.method);
      this.releaseUnusedServerRequestHandler(message.method);
      return true;
    }

    try {
      if (message.type === "appServer.session.initialize") {
        await this.flushWorkspaceRoots();
        await this.ensureInitialized();
        view.post({
          type: "appServer.session.response",
          requestId: message.requestId,
          result: this.initializationResult,
        });
        if (this.latestSessionStatus) {
          view.post({
            type: "appServer.session.status",
            status: serializeSessionStatus(this.latestSessionStatus),
          });
        }
        return true;
      }
      const connection = await this.ensureInitialized();
      if (message.type === "appServer.session.request") {
        const result = await connection.request(
          message.method as ProtocolMethod,
          message.params as never,
          message.meta,
        );
        view.post({ type: "appServer.session.response", requestId: message.requestId, result });
        return true;
      }
      if (message.type === "appServer.session.subscribe") {
        view.subscriptions.get(message.subscriptionId)?.();
        view.subscriptions.set(
          message.subscriptionId,
          connection.subscribeState(message.scope, {
            onSnapshot: (snapshot, event, snapshotChanged) => view.post({
              type: "appServer.session.snapshot",
              subscriptionId: message.subscriptionId,
              snapshot,
              event,
              snapshotChanged,
            }),
            onBaselineLost: () => view.post({
              type: "appServer.session.baselineLost",
              subscriptionId: message.subscriptionId,
            }),
            onBaselineReady: () => view.post({
              type: "appServer.session.baselineReady",
              subscriptionId: message.subscriptionId,
            }),
            onBaselineError: (error) => view.post({
              type: "appServer.session.baselineError",
              subscriptionId: message.subscriptionId,
              error: serializeBridgeError(error),
            }),
          }),
        );
        return true;
      }
      if (message.type === "appServer.session.registerRequestHandler") {
        view.requestMethods.add(message.method);
        this.ensureServerRequestHandler(connection, message.method);
        return true;
      }
    } catch (error) {
      this.logger?.warn("app server view bridge operation failed", {
        operation: message.type,
        error: error instanceof Error ? error.message : String(error),
      });
      if ("requestId" in message && typeof message.requestId === "string") {
        view.post({
          type: "appServer.session.response",
          requestId: message.requestId,
          error: serializeBridgeError(error),
        });
      } else if (message.type === "appServer.session.subscribe") {
        view.post({
          type: "appServer.session.baselineError",
          subscriptionId: message.subscriptionId,
          error: serializeBridgeError(error),
        });
      }
    }
    return true;
  }

  dispose() {
    for (const viewId of [...this.views.keys()]) this.detachView(viewId);
    for (const request of this.pendingServerRequests.values()) {
      request.reject(new Error("VS Code App Server client disposed"));
    }
    this.pendingServerRequests.clear();
    for (const stop of this.serverRequestStops.values()) stop();
    this.serverRequestStops.clear();
    for (const stop of this.sessionStops.splice(0)) stop();
    void this.connection?.close();
    this.connection = undefined;
    this.initialized = undefined;
    this.initializationResult = undefined;
    this.latestSessionStatus = undefined;
    this.reportedWorkspaceRoots = undefined;
    this.workspaceSync = undefined;
  }

  private async ensureInitialized(): Promise<AppServerSession> {
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
      this.bindSessionEvents(connection);
      const initializedWorkspaceRoots = cloneWorkspaceRoots(this.desiredWorkspaceRoots);
      this.initializationResult = await connection.initialize({
        clientInstanceId: this.clientInstanceId,
        shell: { kind: "vscodeExtension", name: "OpenAIDE" },
        requestedSurface: { kind: "home" },
        capabilities: {
          protocol: ["requestResponses", "stableClientRequestIds", "resync"],
          shell: [
            "openExternal",
            "revealFile",
            "resolveFileReveal",
            "pickLocalFile",
            "openTerminal",
            "readSecret",
            "writeSecret",
            "showNotification",
          ],
        },
        workspaceRoots: initializedWorkspaceRoots,
      });
      this.reportedWorkspaceRoots = initializedWorkspaceRoots;
      this.connection = connection;
      this.logger?.info("app server host client initialized", {
        workspaceRootCount: initializedWorkspaceRoots.length,
      });
      return connection;
    })().catch((error) => {
      this.initialized = undefined;
      this.initializationResult = undefined;
      throw error;
    });

    return this.initialized;
  }

  private bindSessionEvents(connection: AppServerSession) {
    this.sessionStops.push(
      connection.handleNotification("app/event", (event) => {
        this.broadcast({ type: "appServer.session.notification", event });
      }),
      connection.handleGenerationInvalidated((event) => {
        this.broadcast({ type: "appServer.session.generationInvalidated", event });
      }),
      connection.handleRecoveryBaseline((event) => {
        this.initializationResult = event.result;
        this.broadcast({ type: "appServer.session.recoveryBaseline", event });
      }),
      connection.handleRecoveryFailed((event) => {
        this.broadcast({
          type: "appServer.session.recoveryFailed",
          event: { ...event, error: serializeBridgeError(event.error) },
        });
      }),
      connection.handleSessionStatus((status) => {
        this.latestSessionStatus = status;
        this.broadcast({ type: "appServer.session.status", status: serializeSessionStatus(status) });
      }),
    );
  }

  private ensureServerRequestHandler(connection: AppServerSession, method: ServerRequestMethod) {
    if (this.serverRequestStops.has(method)) return;
    const stop = connection.handleRequest(method, (params, context) => (
      this.forwardServerRequest(method, params, context) as never
    ));
    this.serverRequestStops.set(method, stop);
  }

  private forwardServerRequest<M extends ServerRequestMethod>(
    method: M,
    params: unknown,
    context: BackendRequestContext,
  ): Promise<ServerRequestResponseResultByMethod[M]> {
    const target = [...this.views.entries()].find(([, view]) => view.requestMethods.has(method));
    if (!target) return Promise.reject(new Error(`No VS Code view handles ${method}`));
    const [viewId, view] = target;
    const requestId = `server-request-${this.nextServerRequestId++}`;
    const result = new Promise<ServerRequestResponseResultByMethod[M]>((resolve, reject) => {
      this.pendingServerRequests.set(requestId, {
        viewId,
        resolve: resolve as (value: unknown) => void,
        reject,
      });
    });
    view.post({
      type: "appServer.session.serverRequest",
      requestId,
      method,
      params,
      context: { requestId: context.requestId, scope: context.scope },
    });
    return result;
  }

  private resolveServerRequest(
    viewId: string,
    message: Extract<AppServerSessionViewMessage, { type: "appServer.session.serverResponse" }>,
  ) {
    const pending = this.pendingServerRequests.get(message.requestId);
    if (!pending || pending.viewId !== viewId) return;
    this.pendingServerRequests.delete(message.requestId);
    if (message.error) pending.reject(new Error(message.error.message));
    else pending.resolve(message.result);
  }

  private releaseUnusedServerRequestHandler(method: ServerRequestMethod) {
    if ([...this.views.values()].some((view) => view.requestMethods.has(method))) return;
    this.serverRequestStops.get(method)?.();
    this.serverRequestStops.delete(method);
  }

  private detachView(viewId: string) {
    const view = this.views.get(viewId);
    if (!view) return;
    this.views.delete(viewId);
    this.logger?.info("app server view detached from host client", { viewCount: this.views.size });
    for (const stop of view.subscriptions.values()) stop();
    for (const method of view.requestMethods) this.releaseUnusedServerRequestHandler(method);
    for (const [requestId, request] of this.pendingServerRequests) {
      if (request.viewId !== viewId) continue;
      this.pendingServerRequests.delete(requestId);
      request.reject(new Error("VS Code view detached during App Server request"));
    }
  }

  private broadcast(message: AppServerSessionHostMessage) {
    for (const view of this.views.values()) view.post(message);
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
