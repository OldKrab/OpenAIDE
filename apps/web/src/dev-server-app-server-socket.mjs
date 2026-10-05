import http from "node:http";

const FORWARDED_HANDSHAKE_HEADERS = [
  "sec-websocket-key",
  "sec-websocket-version",
  "sec-websocket-extensions",
  "sec-websocket-protocol",
];

/**
 * Relays the browser's App Server WebSocket. The shell authenticates the
 * browser before this runs and substitutes its own App Server credential, so
 * the browser never holds the token; after the handshake it only moves bytes,
 * leaving sequencing and resume to the reliable session at both ends.
 */
export function createAppServerSocketProxy({ startAppServer, currentEndpoint, logger }) {
  let nextSocketNumber = 0;
  return async function upgrade(req, socket, head, url) {
    const startedAt = Date.now();
    const context = {
      socket_id: `web-socket-${Date.now()}-${++nextSocketNumber}`,
      connection_id: url.searchParams.get("connectionId") ?? undefined,
    };
    let outcome;
    const finish = (reason, fields = {}) => {
      if (outcome) return;
      outcome = reason;
      const failed = reason !== "browser_closed" && reason !== "app_server_closed";
      logger[failed ? "warn" : "info"](failed ? "web_proxy_socket_failed" : "web_proxy_socket_closed", {
        ...context,
        reason_code: reason,
        duration_ms: Date.now() - startedAt,
        ...fields,
      });
    };
    logger.info("web_proxy_socket_started", context);
    // A browser that leaves while the App Server starts must not be relayed.
    socket.on("error", () => socket.destroy());
    try {
      await startAppServer();
    } catch (error) {
      finish("app_server_unavailable", { error_kind: errorKind(error) });
      reject(socket, 503, "Service Unavailable");
      return;
    }
    const endpoint = currentEndpoint();
    if (!endpoint) {
      finish("app_server_unavailable");
      reject(socket, 503, "Service Unavailable");
      return;
    }
    if (socket.destroyed) {
      finish("browser_closed");
      return;
    }

    const headers = {
      host: endpoint.url.host,
      connection: "Upgrade",
      upgrade: "websocket",
      authorization: `Bearer ${endpoint.authToken}`,
    };
    for (const name of FORWARDED_HANDSHAKE_HEADERS) {
      if (req.headers[name] !== undefined) headers[name] = req.headers[name];
    }
    const proxyReq = http.request({
      hostname: endpoint.url.hostname,
      port: Number(endpoint.url.port),
      path: endpoint.url.pathname + url.search,
      method: "GET",
      headers,
    });
    proxyReq.on("upgrade", (proxyRes, upstream, upstreamHead) => {
      socket.write(`HTTP/1.1 ${proxyRes.statusCode} ${proxyRes.statusMessage}\r\n`);
      for (let index = 0; index < proxyRes.rawHeaders.length; index += 2) {
        socket.write(`${proxyRes.rawHeaders[index]}: ${proxyRes.rawHeaders[index + 1]}\r\n`);
      }
      socket.write("\r\n");
      if (upstreamHead.length) socket.write(upstreamHead);
      if (head.length) upstream.write(head);
      // The session frames are small and latency-bound; never batch them.
      socket.setNoDelay(true);
      upstream.setNoDelay(true);
      logger.info("web_proxy_socket_connected", {
        ...context,
        duration_ms: Date.now() - startedAt,
      });
      const close = (reason) => () => {
        finish(reason, {
          bytes_from_browser: socket.bytesRead,
          bytes_from_app_server: upstream.bytesRead,
        });
        socket.destroy();
        upstream.destroy();
      };
      // An HTTP server socket stays half-open after the peer's FIN, so the
      // end of either direction ends the relay rather than waiting for both.
      for (const event of ["end", "close"]) {
        socket.on(event, close("browser_closed"));
        upstream.on(event, close("app_server_closed"));
      }
      upstream.on("error", () => upstream.destroy());
      upstream.pipe(socket);
      socket.pipe(upstream);
    });
    proxyReq.on("response", (proxyRes) => {
      // The App Server declined the handshake; pass its verdict on unchanged.
      finish("app_server_rejected", { http_status: proxyRes.statusCode });
      proxyRes.resume();
      reject(socket, proxyRes.statusCode ?? 502, proxyRes.statusMessage || "Bad Gateway");
    });
    proxyReq.on("error", (error) => {
      finish("app_server_unreachable", { error_kind: errorKind(error) });
      reject(socket, 502, "Bad Gateway");
    });
    socket.on("close", () => {
      if (outcome) return;
      finish("browser_closed");
      proxyReq.destroy();
    });
    proxyReq.end();
  };
}

function reject(socket, status, statusText) {
  if (socket.destroyed) return;
  socket.end(`HTTP/1.1 ${status} ${statusText}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n`);
}

function errorKind(error) {
  return error instanceof Error && error.name ? error.name : typeof error;
}
