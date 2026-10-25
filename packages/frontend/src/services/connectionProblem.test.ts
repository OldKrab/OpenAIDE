import { APP_SERVER_PROTOCOL_VERSION, AppServerProtocolError, TransportLinkError } from "@openaide/app-server-client";
import { describe, expect, it, vi } from "vitest";
import { carryConnectionProblem, classifyConnectionProblem, probeRefusingServer, sideToUpdate, type ConnectionProblem } from "./connectionProblem";

const client = `${APP_SERVER_PROTOCOL_VERSION.major}.${APP_SERVER_PROTOCOL_VERSION.minor}`;
const refused = () => new AppServerProtocolError({
  error: { code: "incompatibleProtocol", message: "Update OpenAIDE on your computer", recoverable: false },
});
const probe = (protocolVersion: string) => ({ stateRootFingerprint: "root", protocolVersion, appVersion: "0.40.0", lifecycle: "running" as const });

describe("classifyConnectionProblem", () => {
  it("tells an absent App Server from a refusal and from anything else", () => {
    expect(classifyConnectionProblem(new TransportLinkError("interrupted", "App Server WebSocket link ended: open_timeout")))
      .toEqual({ kind: "unreachable", detail: "App Server WebSocket link ended: open_timeout" });
    expect(classifyConnectionProblem(new TypeError("Failed to fetch")).kind).toBe("unreachable");
    expect(classifyConnectionProblem(new TransportLinkError("rejected", "closed 4403")).kind).toBe("failed");
    // The App Server expiring the session answered; it is not an absence.
    expect(classifyConnectionProblem(new TransportLinkError("sessionExpired", "App Server WebSocket link ended: server_closed")).kind).toBe("failed");
    expect(classifyConnectionProblem(new Error("App Server request timed out.")).kind).toBe("failed");
    expect(classifyConnectionProblem(refused())).toEqual({
      kind: "incompatible",
      detail: "Update OpenAIDE on your computer",
      clientProtocol: client,
      update: "unknown",
    });
  });

  it("treats a gateway that got no answer as unreachable and other HTTP answers as failures", () => {
    const http = (status: number) => Object.assign(new TransportLinkError("interrupted", `HTTP ${status}`), { status });
    expect(classifyConnectionProblem(http(502)).kind).toBe("unreachable");
    expect(classifyConnectionProblem(http(500)).kind).toBe("failed");
  });
});

describe("sideToUpdate", () => {
  it("asks the older side to update", () => {
    expect(sideToUpdate({ major: 2, minor: 2 }, { major: 2, minor: 3 })).toBe("computer");
    expect(sideToUpdate({ major: 1, minor: 9 }, { major: 2, minor: 0 })).toBe("computer");
    expect(sideToUpdate({ major: 3, minor: 0 }, { major: 2, minor: 3 })).toBe("app");
  });
});

describe("probeRefusingServer", () => {
  it("adds the refusing App Server's versions and reports the probe", async () => {
    const report = vi.fn();
    const probed = await probeRefusingServer(async () => probe("2.0"), classifyConnectionProblem(refused()), report);
    expect(probed).toMatchObject({ kind: "incompatible", server: { protocol: "2.0", appVersion: "0.40.0" } });
    expect(report).toHaveBeenNthCalledWith(1, "app_server_probe_started", { reason: "incompatible_protocol" });
    expect(report).toHaveBeenNthCalledWith(2, "app_server_probe_completed", expect.objectContaining({
      outcome: "success",
      server_protocol_version: "2.0",
      client_protocol_version: client,
    }));
  });

  it("probes only a refusal, and reports a probe that fails or never answers", async () => {
    const report = vi.fn();
    const unreachable = classifyConnectionProblem(new TypeError("Failed to fetch"));
    await expect(probeRefusingServer(async () => probe("2.0"), unreachable, report)).resolves.toBeUndefined();
    expect(report).not.toHaveBeenCalled();

    await expect(probeRefusingServer(async () => { throw new TypeError("Failed to fetch"); }, classifyConnectionProblem(refused()), report))
      .resolves.toBeUndefined();
    expect(report).toHaveBeenLastCalledWith("app_server_probe_completed", expect.objectContaining({ outcome: "failure", error_name: "TypeError" }));

    vi.useFakeTimers();
    try {
      const pending = probeRefusingServer(() => new Promise(() => undefined), classifyConnectionProblem(refused()), report, 1_000);
      await vi.advanceTimersByTimeAsync(1_000);
      await expect(pending).resolves.toBeUndefined();
      expect(report).toHaveBeenLastCalledWith("app_server_probe_completed", expect.objectContaining({ outcome: "failure", error_name: "timeout" }));
    } finally {
      vi.useRealTimers();
    }
  });
});

describe("carryConnectionProblem", () => {
  const known: ConnectionProblem = { kind: "incompatible", detail: "refused", clientProtocol: client, server: { protocol: "2.1", appVersion: "0.10.0" }, update: "computer" };

  it("keeps a refusal and its versions while retries learn nothing new", () => {
    expect(carryConnectionProblem(known, { kind: "incompatible", detail: "refused", clientProtocol: client, update: "unknown" })).toEqual(known);
    expect(carryConnectionProblem(known, { kind: "failed", detail: "App Server WebSocket link ended: server_closed" })).toBe(known);
  });

  it("lets a dropped link, or a first failure, speak for itself", () => {
    const dropped: ConnectionProblem = { kind: "unreachable", detail: "open_timeout" };
    expect(carryConnectionProblem(known, dropped)).toBe(dropped);
    expect(carryConnectionProblem(undefined, dropped)).toBe(dropped);
    expect(carryConnectionProblem(dropped, { kind: "failed", detail: "x" })).toEqual({ kind: "failed", detail: "x" });
  });
});
