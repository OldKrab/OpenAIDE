import { describe, expect, it, vi } from "vitest";
import { createReliableHttpMessageChannel, type ReliableHttpFetch } from "./reliableHttpChannel";
import { createReliableSessionChannel } from "./reliableSessionChannel";
import type { RpcMessage } from "./rpcPeer";
import {
  TransportLinkError,
  type SequencedFrame,
  type TransportLinkEvents,
  type TransportLinkOpener,
  type TransportLinkResume,
} from "./transportLink";

type OpenedFakeLink = {
  events: TransportLinkEvents;
  resume: TransportLinkResume | undefined;
  sent: SequencedFrame[];
  acknowledged: number[];
  closed: boolean;
};

function fakeOpener(peerReceivedThrough: () => number = () => 0) {
  const links: OpenedFakeLink[] = [];
  const openLink: TransportLinkOpener = async ({ events, resume }) => {
    const opened: OpenedFakeLink = { events, resume, sent: [], acknowledged: [], closed: false };
    links.push(opened);
    return {
      sessionId: "session-1",
      serverId: "server-1",
      peerReceivedThrough: peerReceivedThrough(),
      link: {
        send: (frame) => opened.sent.push(frame),
        acknowledge: (through) => opened.acknowledged.push(through),
        close: () => {
          opened.closed = true;
        },
      },
    };
  };
  return { links, openLink };
}

const notification = (name: string): RpcMessage => ({ jsonrpc: "2.0", method: name, params: {} });

describe("ReliableSessionChannel", () => {
  it("sends frames queued before the link opened, in sequence order", async () => {
    const { links, openLink } = fakeOpener();
    const channel = createReliableSessionChannel({ openLink, connectionId: "client-1" });
    channel.send(notification("first"));
    channel.send(notification("second"));

    await channel.ready();

    expect(links[0]?.sent).toEqual([
      { sequence: 1, message: notification("first") },
      { sequence: 2, message: notification("second") },
    ]);
    channel.close();
    expect(links[0]?.closed).toBe(true);
  });

  it("resumes from its cursor when a link skips a server frame", async () => {
    const { links, openLink } = fakeOpener();
    const channel = createReliableSessionChannel({ openLink, connectionId: "client-1", retryDelayMs: 0 });
    const received: RpcMessage[] = [];
    channel.subscribe((message) => received.push(message));
    await channel.ready();
    const first = links[0] as OpenedFakeLink;

    first.events.frame({ sequence: 1, message: notification("one") });
    first.events.frame({ sequence: 3, message: notification("three") });

    await vi.waitFor(() => expect(links).toHaveLength(2));
    expect(first.closed).toBe(true);
    expect(links[1]?.resume).toEqual({ sessionId: "session-1", serverId: "server-1", receivedThrough: 1 });
    // The abandoned link can no longer deliver into the session.
    first.events.frame({ sequence: 2, message: notification("stale") });
    links[1]?.events.frame({ sequence: 2, message: notification("two") });
    expect(received).toEqual([notification("one"), notification("two")]);
    expect(links[1]?.acknowledged).toEqual([2]);
    channel.close();
  });

  it("keeps a frame until the server acknowledges it and resends it after a lost link", async () => {
    let peerReceivedThrough = 0;
    const { links, openLink } = fakeOpener(() => peerReceivedThrough);
    const channel = createReliableSessionChannel({ openLink, connectionId: "client-1", retryDelayMs: 0 });
    await channel.ready();
    channel.send(notification("first"));
    channel.send(notification("second"));
    links[0]?.events.acknowledged(1);
    peerReceivedThrough = 1;

    links[0]?.events.closed({ kind: "interrupted", error: new Error("lost") });

    await vi.waitFor(() => expect(links[1]?.sent).toEqual([
      { sequence: 2, message: notification("second") },
    ]));
    channel.close();
  });

  it("fails instead of resuming when the server ended the session", async () => {
    const { links, openLink } = fakeOpener();
    const channel = createReliableSessionChannel({ openLink, connectionId: "client-1", retryDelayMs: 0 });
    const errors: unknown[] = [];
    channel.subscribeErrors?.((error) => errors.push(error));
    await channel.ready();
    const expired = new TransportLinkError("sessionExpired", "gone");

    links[0]?.events.closed({ kind: "sessionExpired", error: expired });

    expect(errors).toEqual([expired]);
    expect(() => channel.send(notification("late"))).toThrow(expired);
    expect(links).toHaveLength(1);
    channel.close();
  });

  it("polls over HTTP without a cache entry another window could hold", async () => {
    const pollCache: Array<string | undefined> = [];
    const fetch: ReliableHttpFetch = async (_input, init) => {
      if (init.method === "POST") {
        return {
          ok: true,
          status: 200,
          text: async () => JSON.stringify({ transportVersion: 1, sessionId: "session-1", serverId: "server-1" }),
        };
      }
      pollCache.push(init.cache);
      return new Promise(() => undefined);
    };
    const channel = createReliableHttpMessageChannel({
      endpointUrl: "http://127.0.0.1:4321", connectionId: "client-1", fetch,
    });

    await vi.waitFor(() => expect(pollCache).toEqual(["no-store"]));
    channel.close();
  });
});
