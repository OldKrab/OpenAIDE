import { afterEach, describe, expect, it, vi } from "vitest";
import { connectTerminalStream, type TerminalStreamOptions, type TerminalWebSocket } from "./terminalSockets";

class FakeSocket implements TerminalWebSocket {
  binaryType = "blob";
  readyState = 0;
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onclose: ((event: { code: number }) => void) | null = null;
  onerror: (() => void) | null = null;
  sent: unknown[] = [];
  constructor(readonly url: string) {}
  send(data: string | ArrayBufferLike | Uint8Array) { this.sent.push(data); }
  close() { this.readyState = 3; }
  accept() {
    this.readyState = 1;
    this.onopen?.();
    this.onmessage?.({ data: JSON.stringify({ type: "ready", terminalId: "t1" }) });
  }
  ping() { this.onmessage?.({ data: JSON.stringify({ type: "ping" }) }); }
  output(text: string) { this.onmessage?.({ data: new TextEncoder().encode(text).buffer }); }
  drop(code: number) { this.readyState = 3; this.onclose?.({ code }); }
}

function harness(overrides: Partial<TerminalStreamOptions> = {}) {
  const sockets: FakeSocket[] = [];
  const events: string[] = [];
  const states: string[] = [];
  const output: string[] = [];
  let resets = 0;
  let tokens = 0;
  const attach = vi.fn(async () => ({ terminalId: "t1", token: `token-${++tokens}`, path: "/terminal" }));
  const stream = connectTerminalStream({
    terminalId: "t1",
    initialAttach: { terminalId: "t1", token: "token-0", path: "/terminal" },
    attach,
    socketUrl: () => "ws://127.0.0.1:1/terminal",
    size: () => ({ cols: 80, rows: 24 }),
    onReset: () => { resets += 1; },
    onOutput: (bytes) => output.push(new TextDecoder().decode(bytes)),
    onState: (state) => states.push(state),
    log: (_level, event, fields) => events.push(`${event} ${JSON.stringify(fields)}`),
    createSocket: (url) => {
      const socket = new FakeSocket(url);
      sockets.push(socket);
      return socket;
    },
    backoffMs: [10],
    ...overrides,
  });
  return { stream, sockets, events, states, output, attach, resets: () => resets };
}

afterEach(() => vi.useRealTimers());

describe("Terminal stream", () => {
  it("spends the open token in the hello, then carries input, resize and output", async () => {
    const { stream, sockets, states, output } = harness();
    await vi.waitFor(() => expect(sockets).toHaveLength(1));
    sockets[0].accept();
    expect(JSON.parse(sockets[0].sent[0] as string)).toEqual({ type: "hello", token: "token-0", cols: 80, rows: 24 });
    sockets[0].output("$ ");
    stream.write("ls\r");
    stream.resize(100, 30);
    expect(output).toEqual(["$ "]);
    expect(new TextDecoder().decode(sockets[0].sent[1] as Uint8Array)).toBe("ls\r");
    expect(JSON.parse(sockets[0].sent[2] as string)).toEqual({ type: "resize", cols: 100, rows: 30 });
    expect(states).toEqual(["connecting", "live"]);
  });

  it("reattaches with a fresh token after a lost socket and clears for the replay", async () => {
    vi.useFakeTimers();
    const { sockets, states, attach, resets, events } = harness();
    await vi.waitFor(() => expect(sockets).toHaveLength(1));
    sockets[0].accept();
    sockets[0].drop(1006);
    expect(states.at(-1)).toBe("reconnecting");
    await vi.advanceTimersByTimeAsync(20);
    expect(attach).toHaveBeenCalledTimes(1);
    sockets[1].accept();
    expect(JSON.parse(sockets[1].sent[0] as string).token).toBe("token-1");
    expect(resets()).toBe(2);
    expect(states.at(-1)).toBe("live");
    expect(events.some((event) => event.startsWith("terminal_stream_lost") && event.includes("\"close_code\":1006"))).toBe(true);
    expect(events.join("\n")).not.toMatch(/token-/);
  });

  it("stops when the shell ends and never reconnects after detach", async () => {
    vi.useFakeTimers();
    const ended = harness();
    await vi.waitFor(() => expect(ended.sockets).toHaveLength(1));
    ended.sockets[0].accept();
    ended.sockets[0].drop(1000);
    await vi.advanceTimersByTimeAsync(50);
    expect(ended.states.at(-1)).toBe("ended");
    expect(ended.sockets).toHaveLength(1);

    const left = harness();
    await vi.waitFor(() => expect(left.sockets).toHaveLength(1));
    left.sockets[0].accept();
    left.stream.detach();
    left.sockets[0].drop(1006);
    await vi.advanceTimersByTimeAsync(50);
    expect(left.sockets).toHaveLength(1);
    expect(left.attach).not.toHaveBeenCalled();
  });

  it("abandons a socket that opens but never answers and attaches again", async () => {
    vi.useFakeTimers();
    const { sockets, states, events } = harness({ readyTimeoutMs: 100 });
    await vi.advanceTimersByTimeAsync(0);
    sockets[0].readyState = 1;
    sockets[0].onopen?.();
    await vi.advanceTimersByTimeAsync(120);
    expect(states.at(-1)).toBe("reconnecting");
    expect(events.some((event) => event.includes("ready_timeout"))).toBe(true);
    await vi.advanceTimersByTimeAsync(20);
    expect(sockets).toHaveLength(2);
    sockets[1].accept();
    expect(states.at(-1)).toBe("live");
  });

  it("answers pings and treats a live socket that falls silent as lost", async () => {
    vi.useFakeTimers();
    const { sockets, states, events } = harness({ silenceTimeoutMs: 100 });
    await vi.advanceTimersByTimeAsync(0);
    sockets[0].accept();
    await vi.advanceTimersByTimeAsync(80);
    sockets[0].ping();
    expect(JSON.parse(sockets[0].sent.at(-1) as string)).toEqual({ type: "pong" });
    await vi.advanceTimersByTimeAsync(80);
    expect(states.at(-1)).toBe("live");

    await vi.advanceTimersByTimeAsync(40);
    expect(states.at(-1)).toBe("reconnecting");
    expect(events.some((event) => event.startsWith("terminal_stream_lost") && event.includes("\"reason_code\":\"silent\""))).toBe(true);
    await vi.advanceTimersByTimeAsync(20);
    expect(sockets).toHaveLength(2);
  });

  it("keeps retrying while attach is unavailable", async () => {
    vi.useFakeTimers();
    const attach = vi.fn()
      .mockRejectedValueOnce(new Error("offline"))
      .mockResolvedValue({ terminalId: "t1", token: "later", path: "/terminal" });
    const { sockets, states } = harness({ initialAttach: undefined, attach });
    await vi.advanceTimersByTimeAsync(0);
    expect(states).toEqual(["connecting", "reconnecting"]);
    await vi.advanceTimersByTimeAsync(20);
    expect(sockets).toHaveLength(1);
  });
});
