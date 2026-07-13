import http from "node:http";

/** Proxies Vite HTTP and HMR traffic while preserving the browser-facing origin. */
export function createViteProxy({ port, transformResponse, unavailableMessage }) {
  return {
    async request(req, res, url) {
      let response;
      try {
        response = await fetch(new URL(url.pathname + url.search, `http://127.0.0.1:${port}`), {
          method: req.method,
          headers: headersForFetch(req.headers, `127.0.0.1:${port}`),
          body: requestBody(req),
          duplex: "half",
        });
      } catch (error) {
        if (unavailableMessage && isConnectionFailure(error)) {
          writeText(res, 503, unavailableMessage);
          return;
        }
        throw error;
      }

      let headers = responseHeaders(response);
      let body = Buffer.from(await response.arrayBuffer());
      if (transformResponse) {
        ({ body, headers } = await transformResponse({ body, headers, url }));
      }
      headers["content-length"] = String(body.byteLength);
      res.writeHead(response.status, headers);
      if (req.method === "HEAD") res.end();
      else res.end(body);
    },

    upgrade(req, socket, head, url) {
      const proxyReq = http.request({
        hostname: "127.0.0.1",
        port,
        path: url.pathname + url.search,
        method: req.method,
        headers: upgradeHeaders(req.headers, `127.0.0.1:${port}`),
      });
      proxyReq.on("upgrade", (proxyRes, proxySocket, proxyHead) => {
        socket.write(`HTTP/1.1 ${proxyRes.statusCode} ${proxyRes.statusMessage}\r\n`);
        for (let index = 0; index < proxyRes.rawHeaders.length; index += 2) {
          socket.write(`${proxyRes.rawHeaders[index]}: ${proxyRes.rawHeaders[index + 1]}\r\n`);
        }
        socket.write("\r\n");
        if (proxyHead.length) socket.write(proxyHead);
        if (head.length) proxySocket.write(head);
        proxySocket.pipe(socket);
        socket.pipe(proxySocket);
      });
      proxyReq.on("error", () => socket.destroy());
      proxyReq.end();
    },
  };
}

function requestBody(req) {
  return req.method === "GET" || req.method === "HEAD" ? undefined : req;
}

function headersForFetch(headers, hostHeader) {
  const out = {};
  for (const [key, value] of Object.entries(headers)) {
    if (value === undefined) continue;
    const lower = key.toLowerCase();
    if (lower === "connection" || lower === "content-length" || lower === "host") continue;
    out[key] = Array.isArray(value) ? value.join(", ") : value;
  }
  out.host = hostHeader;
  return out;
}

function upgradeHeaders(headers, hostHeader) {
  const out = headersForFetch(headers, hostHeader);
  out.connection = "Upgrade";
  out.upgrade = headers.upgrade ?? "websocket";
  return out;
}

function responseHeaders(response) {
  const out = {};
  for (const [key, value] of response.headers.entries()) {
    const lower = key.toLowerCase();
    if (lower === "connection" || lower === "etag" || lower === "last-modified" || lower === "transfer-encoding") continue;
    out[key] = value;
  }
  out["cache-control"] = "no-store, max-age=0, must-revalidate";
  out.pragma = "no-cache";
  out.expires = "0";
  return out;
}

function isConnectionFailure(error) {
  const cause = error instanceof Error ? error.cause : undefined;
  return cause && typeof cause === "object" && ["ECONNREFUSED", "ECONNRESET"].includes(cause.code);
}

function writeText(res, status, text) {
  res.writeHead(status, { "content-type": "text/plain; charset=utf-8" });
  res.end(text);
}
