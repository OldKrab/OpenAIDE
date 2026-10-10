import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import { readFile, stat } from "node:fs/promises";
import path from "node:path";
import { promisify } from "node:util";
import zlib from "node:zlib";
import { injectBootstrap, webRoute } from "./dev-server-routes.mjs";

const brotliCompress = promisify(zlib.brotliCompress);
const gzip = promisify(zlib.gzip);

// Revalidate on every load: the build emits unhashed names, so a refreshed
// static root must be observed immediately while unchanged files cost a 304.
const CACHE_CONTROL = "no-cache";
const CONTENT_TYPES = {
  ".css": "text/css; charset=utf-8",
  ".gif": "image/gif",
  ".html": "text/html; charset=utf-8",
  ".ico": "image/x-icon",
  ".jpeg": "image/jpeg",
  ".jpg": "image/jpeg",
  ".js": "text/javascript; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".map": "application/json; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".png": "image/png",
  ".svg": "image/svg+xml",
  ".txt": "text/plain; charset=utf-8",
  ".wasm": "application/wasm",
  ".webmanifest": "application/manifest+json; charset=utf-8",
  ".webp": "image/webp",
  ".woff": "font/woff",
  ".woff2": "font/woff2",
};
const COMPRESSIBLE_EXTENSIONS = new Set([
  ".css", ".html", ".js", ".json", ".map", ".mjs", ".svg", ".txt", ".wasm", ".webmanifest",
]);
// Smaller bodies fit one packet either way; compressing them only costs CPU.
const MIN_COMPRESSIBLE_BYTES = 1024;

/**
 * Serves the built Frontend from a directory. Product routes receive
 * `index.html` with the shell bootstrap injected; every other path is a file
 * under the root. Compressed bodies are kept per file and encoding, so a
 * rebuilt file replaces its own entry and the cache stays bounded by the build.
 */
export function createStaticFrontend({ root, presentation, logger }) {
  const compressed = new Map();

  async function request(req, res, url) {
    if (req.method !== "GET" && req.method !== "HEAD") {
      writeText(res, 405, "Method not allowed");
      return;
    }
    const route = webRoute(url.pathname);
    const filePath = route ? path.join(root, "index.html") : safeStaticPath(root, url.pathname);
    const fileStat = filePath ? await stat(filePath).catch(() => undefined) : undefined;
    if (!fileStat?.isFile()) {
      writeText(res, 404, "Not found");
      return;
    }
    const extension = path.extname(filePath).toLowerCase();
    const contentType = CONTENT_TYPES[extension] ?? "application/octet-stream";

    // The bootstrap depends on the route and presentation, so the injected
    // document is validated by its own bytes instead of the file on disk.
    const document = route && extension === ".html"
      ? Buffer.from(injectBootstrap(await readFile(filePath, "utf8"), route, presentation), "utf8")
      : undefined;
    const etag = document
      ? `W/"${createHash("sha256").update(document).digest("base64url").slice(0, 27)}"`
      : `W/"${fileStat.size.toString(16)}-${Math.trunc(fileStat.mtimeMs).toString(16)}"`;
    const size = document ? document.byteLength : fileStat.size;
    const compressible = COMPRESSIBLE_EXTENSIONS.has(extension) && size >= MIN_COMPRESSIBLE_BYTES;
    const headers = {
      "content-type": contentType,
      "cache-control": CACHE_CONTROL,
      etag,
      ...(compressible ? { vary: "Accept-Encoding" } : {}),
    };
    if (matchesEtag(req.headers["if-none-match"], etag)) {
      res.writeHead(304, headers);
      res.end();
      return;
    }

    const encoding = compressible ? preferredEncoding(req.headers["accept-encoding"]) : undefined;
    if (encoding) {
      const body = await compressedBody({ document, encoding, etag, filePath });
      if (body) {
        res.writeHead(200, {
          ...headers,
          "content-encoding": encoding,
          "content-length": String(body.byteLength),
        });
        res.end(req.method === "HEAD" ? undefined : body);
        return;
      }
    }
    res.writeHead(200, { ...headers, "content-length": String(size) });
    if (req.method === "HEAD") res.end();
    else if (document) res.end(document);
    else createReadStream(filePath).pipe(res);
  }

  async function compressedBody({ document, encoding, etag, filePath }) {
    // Injected documents vary by route; they are small enough to compress per request.
    const key = document ? undefined : `${encoding}:${filePath}`;
    const cached = key ? compressed.get(key) : undefined;
    if (cached?.etag === etag) return cached.body;
    try {
      const source = document ?? await readFile(filePath);
      const body = encoding === "br"
        ? await brotliCompress(source, {
            params: {
              // Quality 5 is close to the maximum ratio for JavaScript at a
              // fraction of the default's cost, which is paid once per build.
              [zlib.constants.BROTLI_PARAM_QUALITY]: 5,
              [zlib.constants.BROTLI_PARAM_SIZE_HINT]: source.byteLength,
            },
          })
        : await gzip(source);
      if (key) compressed.set(key, { etag, body });
      return body;
    } catch (error) {
      // The identity body is still correct, so compression failure only costs bytes.
      logger.warn("web_static_compression_failed", {
        encoding_kind: encoding,
        error_kind: error instanceof Error && error.name ? error.name : typeof error,
      });
      return undefined;
    }
  }

  return {
    request,
    // The built Frontend opens no sockets of its own.
    upgrade(_req, socket) {
      socket.destroy();
    },
  };
}

/** Chooses the best encoding the client accepts, honoring an explicit `q=0`. */
export function preferredEncoding(header) {
  const accepted = new Set();
  for (const part of String(Array.isArray(header) ? header.join(",") : header ?? "").split(",")) {
    const [name, ...parameters] = part.trim().toLowerCase().split(";");
    const quality = parameters.map((parameter) => /^\s*q=([0-9.]+)\s*$/.exec(parameter)?.[1]).find(Boolean);
    if (name && (quality === undefined || Number(quality) > 0)) accepted.add(name);
  }
  if (accepted.has("br")) return "br";
  if (accepted.has("gzip")) return "gzip";
  return undefined;
}

function matchesEtag(header, etag) {
  if (typeof header !== "string") return false;
  return header.split(",").some((candidate) => {
    const value = candidate.trim();
    return value === "*" || value === etag;
  });
}

function safeStaticPath(root, requestPath) {
  let decoded;
  try {
    decoded = decodeURIComponent(requestPath);
  } catch {
    return undefined;
  }
  const normalized = path.normalize(decoded).replace(/^(\.\.[/\\])+/, "");
  const relative = normalized.replace(/^[/\\]+/, "");
  const filePath = path.resolve(root, relative || "index.html");
  if (filePath !== root && !filePath.startsWith(`${root}${path.sep}`)) return undefined;
  return filePath;
}

function writeText(res, status, text) {
  res.writeHead(status, { "content-type": "text/plain; charset=utf-8" });
  res.end(text);
}
