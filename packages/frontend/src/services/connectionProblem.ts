import {
  APP_SERVER_PROTOCOL_VERSION,
  AppServerProtocolError,
  TransportLinkError,
  type ClientProbeResult,
  type ProtocolVersion,
} from "@openaide/app-server-client";

/**
 * Why the Frontend has no usable App Server, in the terms the connection page shows.
 * The Frontend owns this: when the App Server cannot be reached or refuses this client,
 * there is no App Server state to render. The raw error stays in `detail` for the
 * explicit details affordance and never becomes the headline.
 */
export type ConnectionProblem =
  | { kind: "unreachable"; detail: string }
  | {
      kind: "incompatible";
      detail: string;
      /** This client's protocol; the App Server refused it. */
      clientProtocol: string;
      /** Known once `client/probe` answers; absent when the probe failed too. */
      server?: { protocol: string; appVersion: string };
      /** Which side must update; unknown until the probe answers. */
      update: "computer" | "app" | "unknown";
    }
  | { kind: "failed"; detail: string };

/** HTTP answers that mean the App Server, or a gateway in front of it, did not answer. */
const UNAVAILABLE_STATUSES = new Set([502, 503, 504]);

export function classifyConnectionProblem(error: unknown): ConnectionProblem {
  const detail = error instanceof Error ? error.message : "Unable to connect to App Server.";
  if (error instanceof AppServerProtocolError && error.protocolError.code === "incompatibleProtocol") {
    return { kind: "incompatible", detail, clientProtocol: formatProtocol(APP_SERVER_PROTOCOL_VERSION), update: "unknown" };
  }
  if (error instanceof TransportLinkError) {
    // A refused link is an answer, not an absence; the HTTP carrier reports its status.
    const status = (error as { status?: unknown }).status;
    if (typeof status === "number") return UNAVAILABLE_STATUSES.has(status) ? { kind: "unreachable", detail } : { kind: "failed", detail };
    // Only a link that dropped without the App Server's word means nothing answered; a link
    // it ended (rejected, or a session it expired) came from a reachable App Server.
    return error.kind === "interrupted" ? { kind: "unreachable", detail } : { kind: "failed", detail };
  }
  // fetch() rejects with a TypeError when nothing answers at all.
  if (error instanceof TypeError) return { kind: "unreachable", detail };
  return { kind: "failed", detail };
}

/**
 * What the page shows after a retry: a retry that only fails again without new facts keeps
 * what an earlier attempt learned. After a refusal, the App Server ending the next link, or
 * refusing again before the probe answers, says nothing new, so the versions stay; a link
 * that drops without an answer still reads as unreachable.
 */
export function carryConnectionProblem(previous: ConnectionProblem | undefined, next: ConnectionProblem): ConnectionProblem {
  if (previous?.kind !== "incompatible") return next;
  if (next.kind === "failed") return previous;
  if (next.kind === "incompatible" && !next.server && previous.server) return { ...next, server: previous.server, update: previous.update };
  return next;
}

/** Adds what `client/probe` told about the App Server that refused this client. */
export function withProbedServer(problem: ConnectionProblem, probe: ClientProbeResult): ConnectionProblem {
  if (problem.kind !== "incompatible") return problem;
  const server = parseProtocol(probe.protocolVersion);
  return {
    ...problem,
    server: { protocol: probe.protocolVersion, appVersion: probe.appVersion },
    update: server ? sideToUpdate(server, APP_SERVER_PROTOCOL_VERSION) : "unknown",
  };
}

/**
 * The App Server accepts a client of its own major and an equal or older minor
 * (ADR-0062), so a refusal means the older side must update.
 */
export function sideToUpdate(server: ProtocolVersion, client: ProtocolVersion): "computer" | "app" {
  if (server.major !== client.major) return server.major < client.major ? "computer" : "app";
  return server.minor < client.minor ? "computer" : "app";
}

function parseProtocol(value: string): ProtocolVersion | undefined {
  const match = /^(\d+)\.(\d+)$/.exec(value);
  return match ? { major: Number(match[1]), minor: Number(match[2]) } : undefined;
}

function formatProtocol(version: ProtocolVersion) {
  return `${version.major}.${version.minor}`;
}

/** Probe outcomes reported to the shell; metadata only. */
export type ConnectionProbeReport = (event: "app_server_probe_started" | "app_server_probe_completed", fields: Record<string, unknown>) => void;

/**
 * Asks an App Server that refused this client for its versions. Resolves undefined when
 * it cannot say, and the page keeps showing the refusal without versions.
 */
export async function probeRefusingServer(
  probe: (() => Promise<ClientProbeResult>) | undefined,
  problem: ConnectionProblem,
  report: ConnectionProbeReport,
  timeoutMs = 5_000,
): Promise<ConnectionProblem | undefined> {
  if (problem.kind !== "incompatible" || !probe) return undefined;
  const startedAt = Date.now();
  report("app_server_probe_started", { reason: "incompatible_protocol" });
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    const result = await Promise.race([
      probe(),
      new Promise<never>((_, reject) => { timer = setTimeout(() => reject(new Error("probe_timeout")), timeoutMs); }),
    ]);
    const probed = withProbedServer(problem, result);
    report("app_server_probe_completed", {
      outcome: "success",
      duration_ms: Date.now() - startedAt,
      server_protocol_version: result.protocolVersion,
      client_protocol_version: problem.clientProtocol,
      update: probed.kind === "incompatible" ? probed.update : "unknown",
    });
    return probed;
  } catch (error) {
    report("app_server_probe_completed", {
      outcome: "failure",
      duration_ms: Date.now() - startedAt,
      error_name: error instanceof Error && error.message === "probe_timeout" ? "timeout" : error instanceof Error ? error.name : "unknown",
    });
    return undefined;
  } finally {
    if (timer) clearTimeout(timer);
  }
}
