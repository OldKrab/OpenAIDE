import { describe, expect, it, vi } from "vitest";
import {
  createBridgedAppServerSession,
  type AppServerSessionHostMessage,
  type AppServerSessionViewMessage,
} from "./appServerSessionBridge";
import { SECRET_READ, type ClientInstanceId } from "./generated/protocol";

describe("bridged App Server session", () => {
  it("preserves request, subscription, and reverse-request semantics across the shell bridge", async () => {
    const bridge = bridgePort();
    const session = createBridgedAppServerSession(bridge.port);
    const initialization = session.initialize({
      clientInstanceId: "renderer-id" as ClientInstanceId,
      shell: { kind: "vscodeExtension" },
      requestedSurface: { kind: "home" },
    });
    expect(bridge.posted[0]).toMatchObject({
      type: "appServer.session.initialize",
      requestId: "request-1",
    });
    bridge.receive({
      type: "appServer.session.response",
      requestId: "request-1",
      result: initializeResult(),
    });
    await expect(initialization).resolves.toEqual(initializeResult());

    const snapshot = vi.fn();
    const ready = vi.fn();
    session.subscribeState({ kind: "projects" }, {
      onSnapshot: snapshot,
      onBaselineReady: ready,
    });
    bridge.receive({
      type: "appServer.session.snapshot",
      subscriptionId: "subscription-1",
      snapshot: { kind: "projects", projects: { projects: [] } },
    });
    bridge.receive({
      type: "appServer.session.baselineReady",
      subscriptionId: "subscription-1",
    });
    expect(snapshot).toHaveBeenCalledWith(
      { kind: "projects", projects: { projects: [] } },
      undefined,
      undefined,
    );
    expect(ready).toHaveBeenCalledOnce();

    session.handleRequest(SECRET_READ, async ({ key }) => ({ value: `secret:${key}` }));
    bridge.receive({
      type: "appServer.session.serverRequest",
      requestId: "server-request-1",
      method: SECRET_READ,
      params: { key: "agent.token" },
      context: { requestId: "rpc-1" },
    });
    await vi.waitFor(() => expect(bridge.posted).toContainEqual({
      type: "appServer.session.serverResponse",
      requestId: "server-request-1",
      result: { value: "secret:agent.token" },
    }));

    session.close();
    expect(bridge.posted.at(-1)).toEqual({ type: "appServer.session.detach" });
  });
});

function bridgePort() {
  const posted: AppServerSessionViewMessage[] = [];
  let listener: ((message: AppServerSessionHostMessage) => void) | undefined;
  return {
    posted,
    port: {
      post: (message: AppServerSessionViewMessage) => posted.push(message),
      subscribe: (next: (message: AppServerSessionHostMessage) => void) => {
        listener = next;
        return () => {
          listener = undefined;
        };
      },
    },
    receive(message: AppServerSessionHostMessage) {
      listener?.(message);
    },
  };
}

function initializeResult() {
  return {
    snapshot: {
      cursor: "cursor-1",
      server: { serverId: "server-1", protocolVersion: { major: 1, minor: 0 } },
      stateRoot: { stateRootId: "root-1" },
      client: {
        clientInstanceId: "vscode-host-1",
        shellKind: "vscodeExtension" as const,
        surface: { kind: "home" as const },
      },
    },
  };
}
