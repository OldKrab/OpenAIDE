import type { ChildProcessWithoutNullStreams } from "node:child_process";
import * as vscode from "vscode";
import type {
  AppServerSessionHostMessage,
  AppServerStateObserver,
  BackendUnsubscribe,
  ClientWorkspaceRoot,
  ProtocolMethod,
  RequestMeta,
  RequestParamsByMethod,
  ResponseResultByMethod,
  SubscriptionScope,
} from "@openaide/app-server-client";
import type { HealthResult } from "@openaide/app-shell-contracts";
import { ExtensionLogger } from "../logging/logger";
import { runtimeMethods } from "./methods";
import { RuntimeProcess } from "./process";
import { AppServerHostClient } from "./appServerHostClient";
import type { PendingRequest, RuntimeHostRequestHandler, RuntimeNotification } from "./rpcClientTypes";
import { runRuntimeHostRequest } from "./rpcHostRequests";
import { attachRuntimeLineReader } from "./rpcLineReader";
import {
  type RpcId,
  type RuntimeHostRequestMessage,
} from "./rpcWire";
import {
  createRuntimeRequestPayload,
  type RuntimeRequestOptions,
  writeRuntimeHostResponse,
  writeRuntimeJsonLine,
} from "./rpcRequestWriter";
export type { RuntimeHostRequestHandler, RuntimeNotification } from "./rpcClientTypes";

export class RuntimeClient implements vscode.Disposable {
  private nextId = 1;
  private pending = new Map<RpcId, PendingRequest>();
  private notificationListeners = new Set<(notification: RuntimeNotification) => void>();
  private hostRequestHandlers = new Map<string, RuntimeHostRequestHandler>();
  private started = false;
  private starting: Promise<void> | undefined;
  private activeChild: ChildProcessWithoutNullStreams | undefined;
  private readonly appServerHostClient: AppServerHostClient;

  constructor(
    private readonly runtimeProcess: RuntimeProcess,
    private readonly logger: ExtensionLogger,
  ) {
    this.appServerHostClient = new AppServerHostClient(runtimeProcess, logger);
    this.runtimeProcess.onExit(() => {
      this.started = false;
      this.starting = undefined;
      this.activeChild = undefined;
      this.rejectPending(new Error("OpenAIDE App Server stopped"));
    });
  }

  async health() {
    return this.request<HealthResult>(runtimeMethods.health);
  }

  async appServerRequest<M extends ProtocolMethod>(
    method: M,
    params: RequestParamsByMethod[M],
    meta?: RequestMeta,
  ): Promise<ResponseResultByMethod[M]> {
    return this.appServerHostClient.request(method, params, meta);
  }

  async syncWorkspaceRoots(roots: ClientWorkspaceRoot[]) {
    await this.appServerHostClient.syncWorkspaceRoots(roots);
  }

  async subscribeAppServerState(
    scope: SubscriptionScope,
    observer: AppServerStateObserver,
  ): Promise<BackendUnsubscribe> {
    return this.appServerHostClient.subscribeState(scope, observer);
  }

  attachAppServerView(viewId: string, post: (message: AppServerSessionHostMessage) => void) {
    return this.appServerHostClient.attachView(viewId, post);
  }

  handleAppServerViewMessage(viewId: string, message: unknown) {
    return this.appServerHostClient.handleViewMessage(viewId, message);
  }

  dispose() {
    this.rejectPending(new Error("Runtime client disposed"));
    this.notificationListeners.clear();
    this.hostRequestHandlers.clear();
    this.appServerHostClient.dispose();
    this.starting = undefined;
    this.activeChild = undefined;
  }

  onNotification(listener: (notification: RuntimeNotification) => void) {
    this.notificationListeners.add(listener);
    return {
      dispose: () => this.notificationListeners.delete(listener),
    };
  }

  onHostRequest(method: string, handler: RuntimeHostRequestHandler) {
    this.hostRequestHandlers.set(method, handler);
    return {
      dispose: () => {
        if (this.hostRequestHandlers.get(method) === handler) {
          this.hostRequestHandlers.delete(method);
        }
      },
    };
  }

  private async request<T = unknown>(
    method: string,
    params?: unknown,
    options: RuntimeRequestOptions = {},
  ): Promise<T> {
    await this.ensureStarted();
    const child = await this.runtimeProcess.start();
    const id = this.nextId++;
    const payload = createRuntimeRequestPayload(id, method, params, options);

    const promise = new Promise<T>((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error("OpenAIDE App Server request timed out"));
      }, options.timeoutMs ?? 30_000);
      this.pending.set(id, {
        resolve: (value) => {
          clearTimeout(timeout);
          resolve(value as T);
        },
        reject: (error) => {
          clearTimeout(timeout);
          reject(error);
        },
        timeout,
      });
    });

    writeRuntimeJsonLine(child, payload);
    return promise;
  }

  private async ensureStarted() {
    if (this.started) return;
    if (this.starting) return this.starting;

    this.starting = (async () => {
      const child = await this.runtimeProcess.start();
      this.activeChild = child;
      attachRuntimeLineReader(child, {
        pending: this.pending,
        notificationListeners: this.notificationListeners,
        handleHostRequest: (message) => void this.handleHostRequest(message),
        logger: this.logger,
      });
      this.started = true;
    })().finally(() => {
      this.starting = undefined;
    });

    return this.starting;
  }

  private async handleHostRequest(message: RuntimeHostRequestMessage) {
    writeRuntimeHostResponse(
      this.activeChild,
      await runRuntimeHostRequest(message, this.hostRequestHandlers.get(message.method)),
      this.logger,
    );
  }

  private rejectPending(error: Error) {
    for (const pending of this.pending.values()) {
      pending.reject(error);
    }
    this.pending.clear();
  }
}
