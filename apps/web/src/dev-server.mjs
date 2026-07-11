import http from "node:http";
import { spawn } from "node:child_process";
import { createReadStream, existsSync } from "node:fs";
import { mkdir, stat } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { parseAppServerHandoffConnection } from "@openaide/app-server-client";
import { createAppServerManager } from "./app-server-manager.mjs";
import {
  allowedHostNamesFromEnv,
  appServerHeaders,
  authConfigFromEnv,
  isAllowedBrowserOrigin,
  isAllowedHost,
  isAuthorized,
  writeUnauthorized,
} from "./dev-server-auth.mjs";
import { injectBootstrap, webRoute } from "./dev-server-routes.mjs";
import { pipeProxyResponse } from "./dev-server-streams.mjs";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../..");
const host = process.env.OPENAIDE_WEB_HOST ?? "127.0.0.1";
const port = Number(process.env.OPENAIDE_WEB_PORT ?? "5174");
const vitePort = Number(process.env.OPENAIDE_WEB_VITE_PORT ?? "5173");
const staticRoot = process.env.OPENAIDE_WEB_STATIC_ROOT
  ? path.resolve(process.env.OPENAIDE_WEB_STATIC_ROOT)
  : undefined;
const stateRoot = path.resolve(process.env.OPENAIDE_WEB_STATE_ROOT ?? path.join(repoRoot, ".openaide-web-dev", "state"));
const runtimeRoot = path.resolve(process.env.OPENAIDE_WEB_RUNTIME_ROOT ?? path.join(repoRoot, ".openaide-web-dev", "runtime"));
const appServerPath = path.resolve(
  process.env.OPENAIDE_APP_SERVER_PATH
    ?? process.env.OPENAIDE_RUNTIME_PATH
    ?? path.join(repoRoot, "target", "debug", "openaide-app-server"),
);
const allowedHosts = allowedHostNamesFromEnv(process.env.OPENAIDE_WEB_ALLOWED_HOSTS);
const authConfig = authConfigFromEnv();
const instanceLabel = process.env.OPENAIDE_WEB_INSTANCE_LABEL?.trim();
const webPresentation = {
  instanceLabel,
  title: process.env.OPENAIDE_WEB_TITLE?.trim() || (instanceLabel ? `OpenAIDE ${instanceLabel}` : "OpenAIDE"),
};

if (!existsSync(appServerPath)) {
  throw new Error(`OpenAIDE App Server not found at ${appServerPath}. Run npm run app-server:build first.`);
}

await mkdir(stateRoot, { recursive: true });
await mkdir(runtimeRoot, { recursive: true });

const vite = staticRoot ? undefined : spawn(
  process.platform === "win32" ? "npm.cmd" : "npm",
  ["exec", "--workspace", "openaide-frontend", "vite", "--", ...viteArgs()],
  {
    cwd: repoRoot,
    env: {
      ...process.env,
      OPENAIDE_VITE_ALLOWED_HOSTS: [host, ...allowedHosts].filter(Boolean).join(","),
    },
    stdio: ["ignore", "inherit", "inherit"],
  },
);

const appServerManager = createAppServerManager({
  readHandoffConnection,
  spawnAppServer,
});

await startAppServer();

const server = http.createServer(async (req, res) => {
  try {
    if (!isAllowedHost(req.headers.host, allowedHosts)) {
      writeText(res, 403, "Host not allowed");
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

    const url = new URL(req.url ?? "/", `http://${req.headers.host ?? host}`);
    if (url.pathname === "/favicon.ico" || url.pathname === "/favicon.svg") {
      writeFavicon(res);
      return;
    }
    if (url.pathname.startsWith("/__openaide-app-server/")) {
      await proxyAppServer(req, res, url);
      return;
    }
    if (staticRoot) {
      await serveStaticFrontend(req, res, url);
    } else {
      await proxyVite(req, res, url);
    }
  } catch (error) {
    writeText(res, 502, error instanceof Error ? error.message : String(error));
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
      socket.destroy();
      return;
    }
    if (staticRoot) {
      socket.destroy();
    } else {
      proxyViteUpgrade(req, socket, head, url);
    }
  } catch {
    socket.destroy();
  }
});

server.listen(port, host, () => {
  console.log(`OpenAIDE Web dev shell listening on http://${host}:${port}`);
  if (authConfig.enabled) {
    console.log("OpenAIDE Web authentication is enabled.");
  } else {
    console.log("Protect public routes with authentication before exposing this server.");
  }
});

for (const signal of ["SIGINT", "SIGTERM"]) {
  process.once(signal, () => shutdown(signal));
}

function shutdown(signal) {
  server.close();
  vite?.kill(signal);
  appServerManager.currentProcess()?.kill(signal);
  process.exit(signal === "SIGINT" ? 130 : 143);
}

async function proxyVite(req, res, url) {
  const response = await fetch(new URL(url.pathname + url.search, `http://127.0.0.1:${vitePort}`), {
    method: req.method,
    headers: headersForFetch(req.headers, `127.0.0.1:${vitePort}`),
    body: requestBody(req),
    duplex: "half",
  });
  const headers = responseHeaders(response);
  let body = Buffer.from(await response.arrayBuffer());
  const route = webRoute(url.pathname);
  if (route && headers["content-type"]?.includes("text/html")) {
    body = Buffer.from(injectBootstrap(body.toString("utf8"), route, webPresentation), "utf8");
    headers["content-length"] = String(body.byteLength);
  }
  res.writeHead(response.status, headers);
  res.end(body);
}

async function serveStaticFrontend(req, res, url) {
  if (req.method !== "GET" && req.method !== "HEAD") {
    writeText(res, 405, "Method not allowed");
    return;
  }
  const route = webRoute(url.pathname);
  const filePath = route
    ? path.join(staticRoot, "index.html")
    : safeStaticPath(staticRoot, url.pathname);
  if (!filePath) {
    writeText(res, 404, "Not found");
    return;
  }
  let fileStat;
  try {
    fileStat = await stat(filePath);
  } catch {
    writeText(res, 404, "Not found");
    return;
  }
  if (!fileStat.isFile()) {
    writeText(res, 404, "Not found");
    return;
  }
  const headers = {
    "content-type": contentType(filePath),
    "cache-control": "no-store, max-age=0, must-revalidate",
    pragma: "no-cache",
    expires: "0",
  };
  if (route && headers["content-type"].startsWith("text/html")) {
    const html = await import("node:fs/promises").then((fs) => fs.readFile(filePath, "utf8"));
    const body = Buffer.from(injectBootstrap(html, route, webPresentation), "utf8");
    headers["content-length"] = String(body.byteLength);
    res.writeHead(200, headers);
    if (req.method === "HEAD") res.end();
    else res.end(body);
    return;
  }
  headers["content-length"] = String(fileStat.size);
  res.writeHead(200, headers);
  if (req.method === "HEAD") {
    res.end();
    return;
  }
  createReadStream(filePath).pipe(res);
}

function proxyViteUpgrade(req, socket, head, url) {
  const proxyReq = http.request({
    hostname: "127.0.0.1",
    port: vitePort,
    path: url.pathname + url.search,
    method: req.method,
    headers: upgradeHeaders(req.headers, `127.0.0.1:${vitePort}`),
  });
  proxyReq.on("upgrade", (proxyRes, proxySocket, proxyHead) => {
    socket.write(`HTTP/1.1 ${proxyRes.statusCode} ${proxyRes.statusMessage}\r\n`);
    for (let i = 0; i < proxyRes.rawHeaders.length; i += 2) {
      socket.write(`${proxyRes.rawHeaders[i]}: ${proxyRes.rawHeaders[i + 1]}\r\n`);
    }
    socket.write("\r\n");
    if (proxyHead.length) socket.write(proxyHead);
    if (head.length) proxySocket.write(head);
    proxySocket.pipe(socket);
    socket.pipe(proxySocket);
  });
  proxyReq.on("error", () => socket.destroy());
  proxyReq.end();
}

async function proxyAppServer(req, res, url) {
  if (req.method !== "POST" && req.method !== "OPTIONS") {
    writeText(res, 405, "Method not allowed");
    return;
  }
  const body = await readRequestBody(req);
  try {
    await startAppServer();
    await forwardAppServerRequest(req, res, url, body);
  } catch (error) {
    if (!isRestartableAppServerFailure(error)) throw error;
    appServerManager.clearConnection();
    await startAppServer();
    await forwardAppServerRequest(req, res, url, body);
  }
}

function forwardAppServerRequest(req, res, url, body) {
  const appServerConnection = appServerManager.currentConnection();
  const appServerUrl = appServerManager.currentUrl();
  if (!appServerConnection || !appServerUrl) {
    return Promise.reject(new Error("App Server connection is not ready"));
  }
  return new Promise((resolve, reject) => {
    const headers = appServerHeaders(req.headers, appServerUrl.host, appServerConnection.authToken, body.byteLength);
    const proxyReq = http.request({
      hostname: appServerUrl.hostname,
      port: Number(appServerUrl.port),
      path: appServerUrl.pathname + url.search,
      method: req.method,
      headers,
    }, (proxyRes) => {
      res.writeHead(proxyRes.statusCode ?? 502, proxyRes.headers);
      pipeProxyResponse(proxyRes, res).then(resolve, reject);
    });
    proxyReq.on("error", reject);
    proxyReq.end(body);
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
  const webProjectRoots = process.env.OPENAIDE_WEB_PROJECT_ROOTS;
  return spawn(appServerPath, [], {
    cwd: repoRoot,
    env: {
      ...baseEnv,
      ...(webProjectRoots ? { OPENAIDE_PROJECT_ROOTS: webProjectRoots } : {}),
      OPENAIDE_STORAGE_ROOT: stateRoot,
      OPENAIDE_RUNTIME_ROOT: runtimeRoot,
      OPENAIDE_APP_SERVER_PROTOCOL: "app-server-handoff",
    },
    stdio: ["pipe", "pipe", "inherit"],
  });
}

function isRestartableAppServerFailure(error) {
  if (error instanceof Error && error.message === "App Server connection is not ready") return true;
  return error && typeof error === "object" && ["ECONNREFUSED", "ECONNRESET", "EPIPE"].includes(error.code);
}

function readRequestBody(req) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    req.on("data", (chunk) => chunks.push(chunk));
    req.once("end", () => resolve(Buffer.concat(chunks)));
    req.once("error", reject);
  });
}

function viteArgs() {
  const args = ["--host", "127.0.0.1", "--port", String(vitePort)];
  if (process.env.OPENAIDE_VITE_CONFIG) {
    args.push("--config", process.env.OPENAIDE_VITE_CONFIG);
  }
  return args;
}

function safeStaticPath(root, requestPath) {
  const decoded = decodeURIComponent(requestPath);
  const normalized = path.normalize(decoded).replace(/^(\.\.[/\\])+/, "");
  const relative = normalized.replace(/^[/\\]+/, "");
  const filePath = path.resolve(root, relative || "index.html");
  if (filePath !== root && !filePath.startsWith(`${root}${path.sep}`)) return undefined;
  return filePath;
}

function contentType(filePath) {
  switch (path.extname(filePath).toLowerCase()) {
    case ".html":
      return "text/html; charset=utf-8";
    case ".js":
      return "text/javascript; charset=utf-8";
    case ".css":
      return "text/css; charset=utf-8";
    case ".json":
      return "application/json; charset=utf-8";
    case ".svg":
      return "image/svg+xml";
    case ".png":
      return "image/png";
    case ".jpg":
    case ".jpeg":
      return "image/jpeg";
    case ".ico":
      return "image/x-icon";
    default:
      return "application/octet-stream";
  }
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
    if (lower === "connection" || lower === "etag" || lower === "last-modified") continue;
    out[key] = value;
  }
  out["cache-control"] = "no-store, max-age=0, must-revalidate";
  out.pragma = "no-cache";
  out.expires = "0";
  return out;
}

function writeText(res, status, text) {
  res.writeHead(status, { "content-type": "text/plain; charset=utf-8" });
  res.end(text);
}

function writeFavicon(res) {
  const body = Buffer.from(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">
  <rect width="64" height="64" rx="14" fill="#252526"/>
  <path fill="#4d9cff" d="M18 43 29 18h6l11 25h-7l-2.2-5.4h-9.7L25 43h-7Zm11.4-11.3h5.1L32 25.4l-2.6 6.3Z"/>
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
    const timeout = setTimeout(() => reject(new Error("App Server handoff timed out")), 5000);
    child.stdout.on("data", (chunk) => {
      buffer += chunk.toString("utf8");
      const newline = buffer.indexOf("\n");
      if (newline < 0) return;
      clearTimeout(timeout);
      resolve(parseAppServerHandoffConnection(buffer.slice(0, newline)));
    });
    child.once("exit", () => {
      clearTimeout(timeout);
      reject(new Error("App Server exited before handoff"));
    });
    child.once("error", (error) => {
      clearTimeout(timeout);
      reject(error);
    });
  });
}
