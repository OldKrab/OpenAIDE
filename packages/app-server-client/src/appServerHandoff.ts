export type AppServerHandoffConnection = {
  endpointUrl: string;
  authToken: string;
};

export function parseAppServerHandoffConnection(line: string): AppServerHandoffConnection {
  const parsed = JSON.parse(line) as unknown;
  if (!parsed || typeof parsed !== "object") {
    throw new Error("App Server handoff must be an object");
  }
  const record = parsed as Record<string, unknown>;
  if (record.kind !== "localHttp") {
    throw new Error("App Server handoff kind must be localHttp");
  }
  if (typeof record.endpointUrl !== "string" || !isLoopbackEndpoint(record.endpointUrl)) {
    throw new Error("App Server handoff endpoint must be loopback HTTP");
  }
  if (typeof record.authToken !== "string" || record.authToken.length < 32) {
    throw new Error("App Server handoff token is missing or too short");
  }
  return {
    endpointUrl: record.endpointUrl,
    authToken: record.authToken,
  };
}

function isLoopbackEndpoint(endpointUrl: string) {
  try {
    const url = new URL(endpointUrl);
    return url.protocol === "http:" && (url.hostname === "127.0.0.1" || url.hostname === "localhost" || url.hostname === "::1");
  } catch {
    return false;
  }
}
