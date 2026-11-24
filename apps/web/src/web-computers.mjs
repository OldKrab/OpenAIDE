import { createHash } from "node:crypto";
import { ComputerRefusal } from "./computer-gateway.mjs";
import { isComputerId } from "./dev-server-routes.mjs";

const ROUTES_PREFIX = "/__openaide-computers/";
const MAX_BODY_BYTES = 16 * 1024;
// Browsers keep a cookie at most 400 days; a choice older than that falls back to this computer.
const COOKIE_MAX_AGE_S = 400 * 24 * 60 * 60;
const ACTIONS = { list: "GET", use: "POST", pair: "POST", forget: "POST" };
// A page waits this long for the list before it opens on the browser's choice unverified.
const PAGE_CHOICE_WAIT_MS = 5_000;

/**
 * The cookie that keeps a browser's computer (ADR-0082). Cookies ignore ports, so
 * the name is derived from this Web App's folder: two Web Apps on one host keep
 * their choices apart.
 */
export function computerCookieName(folder) {
  return `openaide_computer_${createHash("sha256").update(folder).digest("hex").slice(0, 12)}`;
}

/**
 * The computers a browser can use through this Web App. The browser's choice is
 * a cookie and decides only which computer a page opens on; the page then
 * addresses that computer in its App Server paths, so another tab of the same
 * browser switching does not move it. Using a computer reaches it first, so a
 * browser only moves to a computer that answers (ADR-0070).
 */
export function createWebComputers({ gateway, logger, cookieName, pageChoiceWaitMs = PAGE_CHOICE_WAIT_MS }) {
  return {
    /**
     * The computer a page opens on, and a cookie header to send when the choice
     * names a computer no longer paired here.
     */
    async pageComputer(req) {
      const chosen = readCookie(req.headers.cookie, cookieName);
      if (chosen === undefined) return { computer: undefined };
      if (!isComputerId(chosen)) return { computer: undefined, setCookie: clearedCookie(req, cookieName) };
      let timer;
      try {
        const paired = await Promise.race([
          gateway.computers(),
          new Promise((_resolve, reject) => {
            timer = setTimeout(() => reject(new ComputerRefusal("timeout", "")), pageChoiceWaitMs);
          }),
        ]);
        if (paired.some((computer) => computer.id === chosen)) return { computer: chosen };
        logger.info("web_computer_choice_dropped", { reason: "not_paired" });
        return { computer: undefined, setCookie: clearedCookie(req, cookieName) };
      } catch (error) {
        // Unverified, the choice stands: the page then says why that computer cannot be used.
        logger.warn("web_computer_choice_unverified", { reason: refusalReason(error) });
        return { computer: chosen };
      } finally {
        clearTimeout(timer);
      }
    },

    /** The endpoint for a paired computer; rejects with a `ComputerRefusal`. */
    endpoint: (computer) => gateway.connect(computer),

    /** Forgets an endpoint that stopped answering. */
    release: (computer) => gateway.release(computer),

    /** Answers `/__openaide-computers/<action>`. */
    async handle(req, res, url) {
      const action = url.pathname.slice(ROUTES_PREFIX.length);
      const method = Object.hasOwn(ACTIONS, action) ? ACTIONS[action] : undefined;
      if (!method) {
        writeJson(res, 404, { error: { reason: "not_found", message: "Not found" } });
        return;
      }
      if (req.method !== method) {
        writeJson(res, 405, { error: { reason: "method_not_allowed", message: "Method not allowed" } });
        return;
      }
      const startedAt = Date.now();
      let operationId;
      let target;
      try {
        const body = method === "POST" ? await readJsonBody(req) : {};
        operationId = typeof body.operationId === "string" && body.operationId.length <= 128 ? body.operationId : undefined;
        logger.info("web_computers_request_started", { action, operation_id: operationId });
        let result = {};
        let setCookie;
        switch (action) {
          case "list":
            result = { computers: await gateway.computers() };
            break;
          case "use": {
            const computer = body.computer ?? null;
            target = computer === null ? "this_device" : "computer";
            if (computer === null) {
              setCookie = clearedCookie(req, cookieName);
            } else {
              await gateway.reach(requireComputerId(computer));
              setCookie = chosenCookie(req, cookieName, computer);
            }
            break;
          }
          case "pair": {
            target = "computer";
            const computer = await gateway.pair(requireCode(body.code), optionalInUse(body.inUse));
            result = { computer };
            // A computer just paired is used at once, as on the Desktop App, once it answers.
            // It stays paired when it cannot be reached now.
            await gateway.connect(computer.id);
            setCookie = chosenCookie(req, cookieName, computer.id);
            break;
          }
          case "forget": {
            const computer = requireComputerId(body.computer);
            await gateway.forget(computer);
            if (readCookie(req.headers.cookie, cookieName) === computer) setCookie = clearedCookie(req, cookieName);
            break;
          }
        }
        writeJson(res, 200, result, setCookie);
        logger.info("web_computers_request_completed", {
          action,
          operation_id: operationId,
          target,
          outcome: "ok",
          duration_ms: Date.now() - startedAt,
        });
      } catch (error) {
        const refusal = error instanceof ComputerRefusal
          ? error
          : new ComputerRefusal("internal", "OpenAIDE could not do that. Try again.");
        writeJson(res, refusalStatus(refusal.reason), { error: { reason: refusal.reason, message: refusal.message } });
        logger.warn("web_computers_request_completed", {
          action,
          operation_id: operationId,
          target,
          outcome: "refused",
          reason: refusal.reason,
          ...(error instanceof ComputerRefusal ? {} : { error_kind: errorKind(error) }),
          duration_ms: Date.now() - startedAt,
        });
      }
    },
  };
}

function requireComputerId(value) {
  if (!isComputerId(value)) throw badRequest();
  return value;
}

function requireCode(value) {
  if (typeof value !== "string" || value.length > 4096) throw badRequest();
  return value;
}

function optionalInUse(value) {
  if (value === undefined || value === null) return undefined;
  if (typeof value?.key !== "string" || typeof value?.name !== "string") throw badRequest();
  if (value.key.length > 256 || value.name.length > 256) throw badRequest();
  return { key: value.key, name: value.name };
}

function badRequest() {
  return new ComputerRefusal("bad_request", "OpenAIDE could not read that request.");
}

function refusalStatus(reason) {
  if (reason === "bad_request") return 400;
  if (reason === "gateway_unavailable" || reason === "timeout" || reason === "internal") return 503;
  // The gateway answered and refused: a wrong code, an unreachable computer, a removal.
  return 422;
}

function readJsonBody(req) {
  // A JSON body cannot be sent across origins without a preflight this server never grants.
  if (!String(req.headers["content-type"] ?? "").toLowerCase().startsWith("application/json")) {
    return Promise.reject(badRequest());
  }
  return new Promise((resolve, reject) => {
    const chunks = [];
    let bytes = 0;
    req.on("data", (chunk) => {
      bytes += chunk.byteLength;
      if (bytes > MAX_BODY_BYTES) {
        chunks.length = 0;
        req.pause();
        reject(badRequest());
        return;
      }
      chunks.push(chunk);
    });
    req.once("end", () => {
      try {
        const body = JSON.parse(Buffer.concat(chunks).toString("utf8"));
        resolve(body && typeof body === "object" && !Array.isArray(body) ? body : {});
      } catch {
        reject(badRequest());
      }
    });
    req.once("error", reject);
  });
}

function readCookie(header, name) {
  for (const part of String(header ?? "").split(";")) {
    const separator = part.indexOf("=");
    if (separator < 0) continue;
    if (part.slice(0, separator).trim() === name) return part.slice(separator + 1).trim();
  }
  return undefined;
}

function chosenCookie(req, name, computer) {
  return `${name}=${computer}; Path=/; HttpOnly; SameSite=Strict; Max-Age=${COOKIE_MAX_AGE_S}${secure(req)}`;
}

function clearedCookie(req, name) {
  return `${name}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0${secure(req)}`;
}

/** Behind an HTTPS proxy the cookie is sent only over HTTPS. */
function secure(req) {
  const forwarded = String(req.headers["x-forwarded-proto"] ?? "").split(",", 1)[0].trim().toLowerCase();
  return forwarded === "https" || req.socket?.encrypted ? "; Secure" : "";
}

function writeJson(res, status, body, setCookie) {
  const headers = { "content-type": "application/json; charset=utf-8", "cache-control": "no-store" };
  if (setCookie) headers["set-cookie"] = setCookie;
  res.writeHead(status, headers);
  res.end(JSON.stringify(body));
}

function refusalReason(error) {
  return error instanceof ComputerRefusal ? error.reason : "internal";
}

function errorKind(error) {
  return error instanceof Error && error.name ? error.name : typeof error;
}
