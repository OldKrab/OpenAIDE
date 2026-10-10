import http from "node:http";
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdir } from "node:fs/promises";
import {
  APP_SERVER_HANDOFF_TIMEOUT_MS,
  APP_SERVER_HANDOFF_MAX_LINE_BYTES,
  parseAppServerHandoffConnection,
} from "@openaide/app-server-client";
import { createAppServerManager } from "./app-server-manager.mjs";
import { createAppServerSocketProxy } from "./dev-server-app-server-socket.mjs";
import {
  allowedHostNamesFromEnv,
  appServerHeaders,
  authConfigFromEnv,
  exposureGuardError,
  isAllowedBrowserOrigin,
  isAllowedHost,
  isAuthorized,
  writeUnauthorized,
} from "./dev-server-auth.mjs";
import { appServerTransportRoute, WEB_MANIFEST_PATH, webManifest } from "./dev-server-routes.mjs";
import { pipeProxyResponse, watchPendingProxyResponse } from "./dev-server-streams.mjs";
import { createRuntimeLogger } from "./runtime-logger.mjs";
import { createMobileStatus } from "./mobile-status.mjs";

// Match the App Server JSON ceiling before buffering at the Web boundary.
// General-file uploads use the streaming path and remain exempt.
const MAX_RPC_BODY_BYTES = 10 * 1024 * 1024;
class RequestBodyTooLarge extends Error {}

/** Reads the settings every Web App Shell entry point shares. */
export function webServerEnvironment(env = process.env) {
  const instanceLabel = env.OPENAIDE_WEB_INSTANCE_LABEL?.trim();
  return {
    allowedHosts: allowedHostNamesFromEnv(env.OPENAIDE_WEB_ALLOWED_HOSTS),
    authConfig: authConfigFromEnv(env),
    authHandledUpstream: env.OPENAIDE_WEB_UPSTREAM_AUTH === "1",
    host: env.OPENAIDE_WEB_HOST ?? "127.0.0.1",
    presentation: {
      // HTTP remains selectable for a network path that cannot carry WebSockets.
      appServerTransport: env.OPENAIDE_WEB_TRANSPORT === "http" ? "http" : "webSocket",
      instanceLabel,
      title: env.OPENAIDE_WEB_TITLE?.trim() || (instanceLabel ? `OpenAIDE ${instanceLabel}` : "OpenAIDE"),
    },
    projectRoots: env.OPENAIDE_WEB_PROJECT_ROOTS,
  };
}

/**
 * Runs the Web App Shell: browser-facing checks, the App Server proxy, and the
 * App Server handoff. The entry point chooses what serves the Frontend, so the
 * development and packaged servers share every product-facing route.
 *
 * `frontend` and the optional `prototype` each handle `request` and `upgrade`.
 */
export async function startWebServer({
  allowedHosts,
  appServerCwd,
  appServerPath,
  authConfig,
  authHandledUpstream,
  frontend,
  host,
  logger = createRuntimeLogger("openaide-web-server"),
  name,
  onShutdown,
  port,
  presentation,
  projectRoots,
  prototype,
  runtimeRoot,
  stateRoot,
}) {
  const exposureError = exposureGuardError({ authConfig, authHandledUpstream, host });
  if (exposureError) throw new Error(exposureError);
  if (!existsSync(appServerPath)) {
    throw new Error(`OpenAIDE App Server not found at ${appServerPath}.`);
  }
  await mkdir(stateRoot, { recursive: true });
  await mkdir(runtimeRoot, { recursive: true });

  const appServerManager = createAppServerManager({
    logger,
    readHandoffConnection,
    spawnAppServer,
  });
  const mobileStatus = createMobileStatus(appServerManager.listTasks);
  const appServerSocketProxy = createAppServerSocketProxy({
    startAppServer,
    currentEndpoint() {
      const connection = appServerManager.currentConnection();
      const url = appServerManager.currentUrl();
      return connection && url ? { url, authToken: connection.authToken } : undefined;
    },
    logger,
  });
  const server = http.createServer(async (req, res) => {
    try {
      if (!isAllowedHost(req.headers.host, allowedHosts)) {
        writeText(res, 403, "Host not allowed");
        return;
      }
      const url = new URL(req.url ?? "/", `http://${req.headers.host ?? host}`);
      // Probes intentionally precede browser-origin auth and expose only process state.
      if (url.pathname === "/livez") {
        writeText(res, 200, "live");
        return;
      }
      if (url.pathname === "/readyz") {
        const ready = appServerManager.currentConnection() !== undefined;
        if (!ready) void startAppServer().catch(() => {});
        writeText(res, ready ? 200 : 503, ready ? "ready" : "starting");
        return;
      }
      if (!isAllowedBrowserOrigin(req.headers.origin, req.headers)) {
        writeText(res, 403, "Origin not allowed");
        return;
      }
      if (!isAuthorized(req.headers, authConfig)) {
        writeUnauthorized(res, authConfig);
        return;
      }

      if (url.pathname === "/__openaide-mobile/status") {
        if (req.method !== "GET") {
          writeText(res, 405, "Method not allowed");
          return;
        }
        const status = await mobileStatus();
        res.writeHead(200, { "content-type": "application/json", "cache-control": "no-store" });
        res.end(JSON.stringify(status));
        return;
      }
      if (url.pathname === "/favicon.ico" || url.pathname === "/favicon.svg") {
        writeFavicon(res);
        return;
      }
      if (url.pathname === WEB_MANIFEST_PATH) {
        const body = Buffer.from(JSON.stringify(webManifest(presentation)), "utf8");
        res.writeHead(200, {
          "content-type": "application/manifest+json; charset=utf-8",
          "cache-control": "no-cache",
          "content-length": String(body.byteLength),
        });
        res.end(body);
        return;
      }
      if (url.pathname.startsWith("/__openaide-app-server/")) {
        await proxyAppServer(req, res, url);
        return;
      }
      if (isPrototypePath(url.pathname)) {
        if (!prototype) {
          writeText(res, 404, "Not found");
          return;
        }
        await prototype.request(req, res, url);
        return;
      }
      await frontend.request(req, res, url);
    } catch (error) {
      if (error instanceof RequestBodyTooLarge) {
        res.setHeader("connection", "close");
        writeText(res, 413, "Request body is too large");
      } else {
        writeText(res, 502, error instanceof Error ? error.message : String(error));
      }
    }
  });
  server.on("upgrade", (req, socket, head) => {
    try {
      if (
        !isAllowedHost(req.headers.host, allowedHosts)
        || !isAllowedBrowserOrigin(req.headers.origin, req.headers)
        || !isAuthorized(req.headers, authConfig)
      ) {
        socket.destroy();
        return;
      }
      const url = new URL(req.url ?? "/", `http://${req.headers.host ?? host}`);
      if (url.pathname.startsWith("/__openaide-app-server/")) {
        void appServerSocketProxy(req, socket, head, url).catch(() => socket.destroy());
        return;
      }
      if (isPrototypePath(url.pathname)) {
        if (prototype) prototype.upgrade(req, socket, head, url);
        else socket.destroy();
        return;
      }
      frontend.upgrade(req, socket, head, url);
    } catch {
      socket.destroy();
    }
  });

  server.listen(port, host, () => {
    // Port 0 leaves the choice to the OS, so report the port actually bound.
    const boundPort = server.address().port;
    console.log(`${name} listening on http://${host}:${boundPort}`);
    logger.info("web_server_listening", {
      port: boundPort,
      host,
      auth_status: authConfig.enabled ? "password" : authHandledUpstream ? "upstream" : "none",
    });
    if (authConfig.enabled) {
      console.log("OpenAIDE Web authentication is enabled.");
    } else {
      console.log("Protect public routes with authentication before exposing this server.");
    }
  });

  // Asset serving and liveness do not depend on product-state recovery. The manager
  // coalesces this eager attempt with the first protocol request and permits retry.
  void startAppServer().catch(() => {});

  for (const signal of ["SIGINT", "SIGTERM"]) {
    process.once(signal, () => shutdown(signal));
  }

  function shutdown(signal) {
    server.close();
    onShutdown?.(signal);
    appServerManager.currentProcess()?.kill(signal);
    process.exit(signal === "SIGINT" ? 130 : 143);
  }

  async function proxyAppServer(req, res, url) {
    const requestId = `web-proxy-${Date.now()}-${Math.random().toString(36).slice(2)}`;
    const connectionId = typeof req.headers["x-openaide-connection-id"] === "string"
      ? req.headers["x-openaide-connection-id"]
      : undefined;
    const transportRoute = appServerTransportRoute(req.method, url.pathname);
    const isUpload = transportRoute?.kind === "upload";
    const isReliablePoll = req.method === "GET"
      && transportRoute === undefined
      && typeof req.headers["x-openaide-session-id"] === "string";
    const transportOperationKind = isReliablePoll
      ? "receive"
      : transportRoute?.kind ?? "rpc";
    const startedAt = Date.now();
    if (req.method !== "GET" && req.method !== "POST" && req.method !== "OPTIONS") {
      writeText(res, 405, "Method not allowed");
      return;
    }
    if (!isReliablePoll) {
      logger.info("web_proxy_request_started", {
        request_id: requestId,
        connection_id: connectionId,
        method: req.method,
        route: url.pathname,
        transport_operation_kind: transportOperationKind,
      });
    }
    try {
      await startAppServer();
      const body = req.method === "POST" && !isUpload ? await readRequestBody(req) : Buffer.alloc(0);
      // An ambiguous proxy failure may happen after App Server acceptance. Only the
      // sequenced client transport may retry the identical frame; the proxy must
      // never manufacture a second application delivery.
      await forwardAppServerRequest(req, res, url, body, transportRoute);
      if (!isReliablePoll || res.statusCode !== 204) {
        logger.info("web_proxy_request_completed", {
          request_id: requestId,
          connection_id: connectionId,
          method: req.method,
          route: url.pathname,
          transport_operation_kind: transportOperationKind,
          http_status: res.statusCode,
          body_bytes: body.byteLength,
          duration_ms: Date.now() - startedAt,
        });
      }
    } catch (error) {
      logger.warn("web_proxy_request_failed", {
        request_id: requestId,
        connection_id: connectionId,
        method: req.method,
        route: url.pathname,
        transport_operation_kind: transportOperationKind,
        duration_ms: Date.now() - startedAt,
        error_kind: error instanceof Error && error.name ? error.name : typeof error,
      });
      throw error;
    }
  }

  function forwardAppServerRequest(req, res, url, body, transportRoute) {
    const appServerConnection = appServerManager.currentConnection();
    const appServerUrl = appServerManager.currentUrl();
    if (!appServerConnection || !appServerUrl) {
      return Promise.reject(new Error("App Server connection is not ready"));
    }
    return new Promise((resolve, reject) => {
      const isUpload = transportRoute?.kind === "upload";
      const contentLength = isUpload ? Number(req.headers["content-length"] ?? 0) : body.byteLength;
      const headers = appServerHeaders(req.headers, appServerUrl.host, appServerConnection.authToken, contentLength);
      const upstreamPath = transportRoute
        ? `${appServerUrl.pathname.replace(/\/$/, "")}/${transportRoute.appServerSuffix}`
        : appServerUrl.pathname;
      const proxyReq = http.request({
        hostname: appServerUrl.hostname,
        port: Number(appServerUrl.port),
        path: upstreamPath + url.search,
        method: req.method,
        headers,
      }, (proxyRes) => {
        if (!pendingResponse.handoff()) {
          proxyRes.destroy();
          return;
        }
        res.writeHead(proxyRes.statusCode ?? 502, proxyRes.headers);
        pipeProxyResponse(proxyRes, res).then(resolve, reject);
      });
      const pendingResponse = watchPendingProxyResponse(proxyReq, res);
      void pendingResponse.cancelled.then(resolve);
      proxyReq.on("error", (error) => {
        pendingResponse.handoff();
        reject(error);
      });
      if (isUpload) req.pipe(proxyReq);
      else proxyReq.end(body);
    });
  }

  async function startAppServer() {
    await appServerManager.startAppServer();
  }

  function spawnAppServer() {
    const {
      OPENAIDE_PROJECT_ROOTS: _projectRoots,
      ...baseEnv
    } = process.env;
    return spawn(appServerPath, [], {
      cwd: appServerCwd,
      env: {
        ...baseEnv,
        ...(projectRoots ? { OPENAIDE_PROJECT_ROOTS: projectRoots } : {}),
        OPENAIDE_STORAGE_ROOT: stateRoot,
        OPENAIDE_RUNTIME_ROOT: runtimeRoot,
        OPENAIDE_APP_SERVER_PROTOCOL: "app-server-handoff",
      },
      stdio: ["pipe", "pipe", "inherit"],
    });
  }

  return server;
}

function readRequestBody(req) {
  return new Promise((resolve, reject) => {
    if (Number(req.headers["content-length"]) > MAX_RPC_BODY_BYTES) {
      reject(new RequestBodyTooLarge());
      return;
    }
    const chunks = [];
    let bytes = 0;
    req.on("data", (chunk) => {
      bytes += chunk.byteLength;
      if (bytes > MAX_RPC_BODY_BYTES) {
        // Stop retaining chunks immediately; the 413 response closes the
        // connection, including requests sent with chunked transfer encoding.
        chunks.length = 0;
        req.pause();
        reject(new RequestBodyTooLarge());
        return;
      }
      chunks.push(chunk);
    });
    req.once("end", () => resolve(Buffer.concat(chunks)));
    req.once("error", reject);
  });
}

function writeText(res, status, text) {
  res.writeHead(status, { "content-type": "text/plain; charset=utf-8" });
  res.end(text);
}

function isPrototypePath(pathname) {
  return pathname === "/prototype" || pathname.startsWith("/prototype/");
}

function writeFavicon(res) {
  const body = Buffer.from(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">
  <g transform="translate(4 4) scale(2.333333)" color="#4d9cff">
    <path d="M12 2V7M12 17V22M2 12H7M17 12H22" fill="none" stroke="currentColor" stroke-width="2.75" stroke-linecap="square" stroke-linejoin="bevel"/>
    <path d="M4.2 4.2L7.8 7.8M16.2 7.8L19.8 4.2M16.2 16.2L19.8 19.8M7.8 16.2L4.2 19.8" fill="none" stroke="currentColor" stroke-width="2.75" stroke-linecap="square" stroke-linejoin="bevel"/>
    <path d="M12 7L17 12L12 17L7 12L12 7Z" fill="currentColor"/>
  </g>
</svg>`, "utf8");
  res.writeHead(200, {
    "content-type": "image/svg+xml; charset=utf-8",
    "cache-control": "no-store, max-age=0, must-revalidate",
    "content-length": String(body.byteLength),
  });
  res.end(body);
}

function readHandoffConnection(child) {
  return new Promise((resolve, reject) => {
    let buffer = "";
    const cleanup = () => {
      clearTimeout(timeout);
      child.stdout.off("data", onData);
      child.off("exit", onExit);
      child.off("error", fail);
    };
    const fail = (error) => {
      cleanup();
      reject(error);
    };
    const onExit = () => fail(new Error("App Server exited before handoff"));
    const onData = (chunk) => {
      buffer += chunk.toString("utf8");
      if (Buffer.byteLength(buffer, "utf8") > APP_SERVER_HANDOFF_MAX_LINE_BYTES) {
        fail(new Error("App Server handoff connection info is too large"));
        return;
      }
      const newline = buffer.indexOf("\n");
      if (newline < 0) return;
      // Parse errors occur in a stream callback, outside the Promise executor.
      // Reject the handoff so the manager can clean up and offer a later retry.
      try {
        const connection = parseAppServerHandoffConnection(buffer.slice(0, newline));
        cleanup();
        resolve(connection);
      } catch (error) {
        fail(error);
      }
    };
    const timeout = setTimeout(
      () => fail(new Error("App Server handoff timed out")),
      APP_SERVER_HANDOFF_TIMEOUT_MS,
    );
    child.stdout.on("data", onData);
    child.once("exit", onExit);
    child.once("error", fail);
  });
}
