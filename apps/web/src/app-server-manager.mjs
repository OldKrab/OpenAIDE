import {
  CLIENT_HEARTBEAT,
  CLIENT_INITIALIZE,
} from "@openaide/app-server-client";

const WEB_SHELL_CLIENT_ID = "web-app-shell";
const WEB_SHELL_CONNECTION_ID = "web-app-shell";
const DEFAULT_SHELL_HEARTBEAT_INTERVAL_MS = 5_000;

export function createAppServerManager({
  connectionUrl = defaultConnectionUrl,
  readHandoffConnection,
  requestAppServer = defaultRequestAppServer,
  shellHeartbeatIntervalMs = DEFAULT_SHELL_HEARTBEAT_INTERVAL_MS,
  spawnAppServer,
}) {
  let appServer;
  let appServerConnection;
  let appServerUrl;
  let shellHeartbeat;
  let startPromise;

  async function startAppServer() {
    if (appServerConnection) {
      return { appServer, appServerConnection, appServerUrl };
    }
    if (startPromise) return startPromise;

    startPromise = startAppServerProcess()
      .finally(() => {
        startPromise = undefined;
      });
    return startPromise;
  }

  async function startAppServerProcess() {
    const child = spawnAppServer();
    appServer = child;
    child.once("exit", () => {
      if (appServer === child) {
        appServer = undefined;
      }
    });

    try {
      const connection = await readHandoffConnection(child);
      await initializeShellClient(connection);
      appServerConnection = connection;
      appServerUrl = connectionUrl(connection);
      return { appServer, appServerConnection, appServerUrl };
    } catch (error) {
      if (appServer === child) {
        stopShellClient();
        appServer = undefined;
        appServerConnection = undefined;
        appServerUrl = undefined;
      }
      if (!child.killed) child.kill();
      throw error;
    }
  }

  async function initializeShellClient(connection) {
    stopShellClient();
    await sendShellRequest(connection, CLIENT_INITIALIZE, {
      clientInstanceId: WEB_SHELL_CLIENT_ID,
      shell: { kind: "web" },
      requestedSurface: { kind: "home" },
      capabilities: {
        protocol: ["requestResponses", "stableClientRequestIds", "resync"],
        shell: [],
      },
    });
    shellHeartbeat = setInterval(() => {
      void sendShellRequest(connection, CLIENT_HEARTBEAT, {}).catch(() => {
        // Browser requests surface App Server failures. The shell heartbeat is
        // only a lifetime keepalive and will retry on the next interval.
      });
    }, shellHeartbeatIntervalMs);
    shellHeartbeat.unref?.();
  }

  function stopShellClient() {
    if (shellHeartbeat !== undefined) {
      clearInterval(shellHeartbeat);
      shellHeartbeat = undefined;
    }
  }

  function sendShellRequest(connection, method, params) {
    return requestAppServer(connection, WEB_SHELL_CONNECTION_ID, {
      jsonrpc: "2.0",
      id: `web-shell-${Date.now()}-${Math.random().toString(36).slice(2)}`,
      method,
      params,
    });
  }

  return {
    clearConnection: () => {
      stopShellClient();
      appServerConnection = undefined;
      appServerUrl = undefined;
      appServer = undefined;
    },
    currentConnection: () => appServerConnection,
    currentProcess: () => appServer,
    currentUrl: () => appServerUrl,
    startAppServer,
  };
}

function defaultConnectionUrl(connection) {
  return new URL(connection.endpointUrl);
}

async function defaultRequestAppServer(connection, connectionId, body) {
  const response = await fetch(connection.endpointUrl, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${connection.authToken}`,
      "Content-Type": "application/json",
      "X-OpenAIDE-Connection-Id": connectionId,
    },
    body: JSON.stringify(body),
  });
  const text = await response.text();
  if (!response.ok) {
    throw new Error(`App Server shell request failed with HTTP ${response.status}: ${text}`);
  }
  const messages = JSON.parse(text);
  const responseMessage = Array.isArray(messages)
    ? messages.find((message) => message?.id === body.id)
    : messages;
  if (responseMessage?.error) {
    throw new Error(responseMessage.error.message ?? "App Server shell request failed");
  }
  return responseMessage?.result;
}
