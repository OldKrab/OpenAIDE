import { timingSafeEqual } from "node:crypto";

export function allowedHostNamesFromEnv(value) {
  return (value ?? "")
    .split(",")
    .map((item) => normalizedHostName(item.trim()))
    .filter(Boolean);
}

export function authConfigFromEnv(env = process.env) {
  const password = env.OPENAIDE_WEB_PASSWORD ?? "";
  if (!password) return { enabled: false };
  return {
    enabled: true,
    realm: env.OPENAIDE_WEB_AUTH_REALM || "OpenAIDE demo",
    username: env.OPENAIDE_WEB_USERNAME || "demo",
    password,
  };
}

export function isAuthorized(headers, config) {
  if (!config.enabled) return true;
  const header = Array.isArray(headers.authorization)
    ? headers.authorization[0]
    : headers.authorization;
  const credentials = parseBasicAuthorization(header);
  if (!credentials) return false;
  return secureEqual(credentials.username, config.username)
    && secureEqual(credentials.password, config.password);
}

export function isAllowedBrowserOrigin(origin, headers) {
  if (!origin) return true;
  const expectedOrigin = expectedBrowserOrigin(headers);
  if (!expectedOrigin) return false;
  try {
    return new URL(origin).origin === expectedOrigin;
  } catch {
    return false;
  }
}

export function isAllowedHost(header, allowedHosts) {
  const hostname = normalizedHostName(header);
  return hostname === "127.0.0.1" || hostname === "localhost" || allowedHosts.includes(hostname);
}

/** Parses one HTTP authority and returns its canonical hostname for exact allowlist matching. */
export function normalizedHostName(authority) {
  if (typeof authority !== "string" || !authority || authority.trim() !== authority) return undefined;
  let parsed;
  try {
    parsed = new URL(`http://${authority}`);
  } catch {
    return undefined;
  }
  if (parsed.username || parsed.password || parsed.pathname !== "/" || parsed.search || parsed.hash) {
    return undefined;
  }
  const hostname = parsed.hostname
    .replace(/^\[|\]$/g, "")
    .replace(/\.$/, "")
    .toLowerCase();
  return hostname || undefined;
}

export function appServerHeaders(headers, hostHeader, authToken, contentLength) {
  const out = {};
  for (const [key, value] of Object.entries(headers)) {
    if (value === undefined) continue;
    const lower = key.toLowerCase();
    if (
      lower === "connection" ||
      lower === "host" ||
      lower === "authorization" ||
      lower === "content-length"
    ) continue;
    out[key] = value;
  }
  out.host = hostHeader;
  out.Authorization = `Bearer ${authToken}`;
  out["Content-Length"] = String(contentLength);
  return out;
}

export function writeUnauthorized(res, config) {
  res.writeHead(401, {
    "content-type": "text/plain; charset=utf-8",
    "www-authenticate": `Basic realm="${escapeRealm(config.realm)}", charset="UTF-8"`,
    "cache-control": "no-store",
  });
  res.end("Authentication required");
}

function parseBasicAuthorization(header) {
  if (!header?.startsWith("Basic ")) return undefined;
  let decoded;
  try {
    decoded = Buffer.from(header.slice("Basic ".length), "base64").toString("utf8");
  } catch {
    return undefined;
  }
  const separator = decoded.indexOf(":");
  if (separator < 0) return undefined;
  return {
    username: decoded.slice(0, separator),
    password: decoded.slice(separator + 1),
  };
}

function expectedBrowserOrigin(headers) {
  const host = firstHeaderValue(headers.host);
  if (!host) return undefined;
  const forwardedProtocol = firstHeaderValue(headers["x-forwarded-proto"])
    ?.split(",", 1)[0]
    ?.trim()
    .toLowerCase();
  const protocol = forwardedProtocol || "http";
  if (protocol !== "http" && protocol !== "https") return undefined;
  try {
    return new URL(`${protocol}://${host}`).origin;
  } catch {
    return undefined;
  }
}

function firstHeaderValue(value) {
  return Array.isArray(value) ? value[0] : value;
}

function secureEqual(left, right) {
  const leftBytes = Buffer.from(left);
  const rightBytes = Buffer.from(right);
  if (leftBytes.length !== rightBytes.length) return false;
  return timingSafeEqual(leftBytes, rightBytes);
}

function escapeRealm(realm) {
  return realm.replaceAll("\\", "\\\\").replaceAll('"', '\\"');
}
