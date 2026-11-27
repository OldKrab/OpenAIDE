import { describe, expect, it } from "vitest";
import {
  createRpcPeer,
  type RpcMessage,
  type RpcNotificationMap,
  type RpcRequestMap,
} from "./rpcPeer";

type Requests = RpcRequestMap & {
  "math/add": { params: { left: number; right: number }; result: number };
};

type Notifications = RpcNotificationMap & {
  "status/changed": { params: { ready: boolean } };
};

describe("RpcPeer", () => {
  it("turns an incoming request handler result into the caller response", async () => {
    const clientInbound: Array<(message: RpcMessage) => void> = [];
    const serverInbound: Array<(message: RpcMessage) => void> = [];
    const client = createRpcPeer<Requests, Notifications, Requests, Notifications>({
      send: (message) => serverInbound.forEach((receive) => receive(message)),
      subscribe: (receive) => subscribe(clientInbound, receive),
    });
    const server = createRpcPeer<Requests, Notifications, Requests, Notifications>({
      send: (message) => clientInbound.forEach((receive) => receive(message)),
      subscribe: (receive) => subscribe(serverInbound, receive),
    });
    let handledRequestId: string | number | undefined;
    server.handleRequest("math/add", ({ left, right }, context) => {
      handledRequestId = context.requestId;
      return left + right;
    });

    await expect(client.request("math/add", { left: 20, right: 22 })).resolves.toBe(42);
    expect(handledRequestId).toBe("rpc-1");
  });

  it("rejects pending requests when the channel reports terminal loss", async () => {
    let reportError: ((error: unknown) => void) | undefined;
    const peer = createRpcPeer<Requests, Notifications, Requests, Notifications>({
      send: () => undefined,
      subscribe: () => () => undefined,
      subscribeErrors: (listener) => {
        reportError = listener;
        return () => undefined;
      },
    });
    const pending = peer.request("math/add", { left: 20, right: 22 });

    reportError?.(new Error("App Server generation ended"));

    await expect(pending).rejects.toThrow("App Server generation ended");
  });

  // App Server answers read-only requests (file viewer reads, task/image) when their work
  // ends, so a reply may arrive after the replies of requests sent later.
  it("matches replies to requests by id when they arrive out of request order", async () => {
    const sent: RpcMessage[] = [];
    let deliver: (message: RpcMessage) => void = () => undefined;
    const peer = createRpcPeer<Requests, Notifications, Requests, Notifications>({
      send: (message) => sent.push(message),
      subscribe: (receive) => {
        deliver = receive;
        return () => undefined;
      },
    });
    const settled: number[] = [];
    const first = peer.request("math/add", { left: 1, right: 1 }).then((value) => settled.push(value));
    const second = peer.request("math/add", { left: 2, right: 2 }).then((value) => settled.push(value));
    const third = peer.request("math/add", { left: 3, right: 3 }).then((value) => settled.push(value));
    const [a, b, c] = sent.map((message) => ("id" in message ? message.id : undefined));
    if (a === undefined || b === undefined || c === undefined) throw new Error("requests were not sent");

    deliver({ jsonrpc: "2.0", id: c, result: 6 });
    deliver({ jsonrpc: "2.0", id: a, result: 2 });
    deliver({ jsonrpc: "2.0", id: b, result: 4 });
    await Promise.all([first, second, third]);

    expect(settled).toEqual([6, 2, 4]);
  });
});

function subscribe(
  listeners: Array<(message: RpcMessage) => void>,
  listener: (message: RpcMessage) => void,
) {
  listeners.push(listener);
  return () => {
    const index = listeners.indexOf(listener);
    if (index !== -1) listeners.splice(index, 1);
  };
}
