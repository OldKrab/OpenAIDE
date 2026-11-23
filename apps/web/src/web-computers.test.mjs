import assert from "node:assert/strict";
import http from "node:http";
import test from "node:test";
import { ComputerRefusal } from "./computer-gateway.mjs";
import { appServerBase, appServerTarget, injectBootstrap } from "./dev-server-routes.mjs";
import { computerCookieName, createWebComputers } from "./web-computers.mjs";

const DESK = "d".repeat(64);
const LAPTOP = "e".repeat(64);
const COOKIE = computerCookieName("/state/web-app");

/** A gateway with a list, which refuses to connect to the computers in `unreachable`. */
function fakeGateway({ computers = [{ id: DESK, name: "Desk" }], unreachable = new Set(), failList = false } = {}) {
  const calls = [];
  return {
    calls,
    async computers() {
      calls.push(["computers"]);
      if (failList) throw new ComputerRefusal("gateway_unavailable", "OpenAIDE could not reach other computers. Try again.");
      return computers;
    },
    async pair(code, inUse) {
      calls.push(["pair", code, inUse]);
      if (code === "own") throw new ComputerRefusal("own_code", "This is Desk's own code.");
      computers = [...computers, { id: LAPTOP, name: "Laptop" }];
      return { id: LAPTOP, name: "Laptop" };
    },
    async connect(id) {
      calls.push(["connect", id]);
      if (unreachable.has(id)) throw new ComputerRefusal("unreachable", "That computer is unreachable.");
      return { url: new URL("http://127.0.0.1:1/probe"), authToken: "t" };
    },
    release() {},
    async forget(id) {
      calls.push(["forget", id]);
      computers = computers.filter((computer) => computer.id !== id);
    },
  };
}

function recordingLogger() {
  const events = [];
  const record = (level) => (event, fields = {}) => events.push({ level, event, fields });
  return { events, logger: { info: record("info"), warn: record("warn"), error: record("error") } };
}

async function serve(t, computers) {
  const server = http.createServer((req, res) => {
    void computers.handle(req, res, new URL(req.url, "http://localhost"));
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  t.after(() => new Promise((resolve) => server.close(resolve)));
  const origin = `http://127.0.0.1:${server.address().port}`;
  return (action, body, headers = {}) => fetch(`${origin}/__openaide-computers/${action}`, body === undefined
    ? { headers }
    : { method: "POST", headers: { "content-type": "application/json", ...headers }, body: JSON.stringify(body) });
}

test("using a computer reaches it first, then keeps the choice in a cookie", async (t) => {
  const gateway = fakeGateway({ unreachable: new Set([LAPTOP]) });
  const { events, logger } = recordingLogger();
  const request = await serve(t, createWebComputers({ gateway, logger, cookieName: COOKIE }));

  const used = await request("use", { computer: DESK, operationId: "switch-1" });
  assert.equal(used.status, 200);
  const cookie = used.headers.get("set-cookie");
  assert.match(cookie, new RegExp(`^${COOKIE}=${DESK}; Path=/; HttpOnly; SameSite=Strict; Max-Age=\\d+$`));
  assert.deepEqual(gateway.calls.at(-1), ["connect", DESK]);

  // A computer that does not answer leaves the browser where it was.
  const refused = await request("use", { computer: LAPTOP });
  assert.equal(refused.status, 422);
  assert.equal(refused.headers.get("set-cookie"), null);
  assert.deepEqual(await refused.json(), { error: { reason: "unreachable", message: "That computer is unreachable." } });

  const back = await request("use", { computer: null });
  assert.match(back.headers.get("set-cookie"), new RegExp(`^${COOKIE}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0$`));

  const switched = events.filter((entry) => entry.event === "web_computers_request_completed");
  assert.deepEqual(switched.map((entry) => [entry.fields.target, entry.fields.outcome, entry.fields.reason]), [
    ["computer", "ok", undefined],
    ["computer", "refused", "unreachable"],
    ["this_device", "ok", undefined],
  ]);
  assert.equal(switched[0].fields.operation_id, "switch-1");
  assert.equal(JSON.stringify(events).includes(DESK), false, "computer ids stay out of the log");
});

test("pairing spends the code, then uses the computer once it answers", async (t) => {
  const gateway = fakeGateway();
  const request = await serve(t, createWebComputers({ gateway, logger: recordingLogger().logger, cookieName: COOKIE }));

  const paired = await request("pair", { code: "OAI1CODE", inUse: { key: DESK, name: "Desk" } });
  assert.equal(paired.status, 200);
  assert.deepEqual(await paired.json(), { computer: { id: LAPTOP, name: "Laptop" } });
  assert.match(paired.headers.get("set-cookie"), new RegExp(`^${COOKIE}=${LAPTOP};`));
  assert.deepEqual(gateway.calls.slice(0, 2), [["pair", "OAI1CODE", { key: DESK, name: "Desk" }], ["connect", LAPTOP]]);

  const own = await request("pair", { code: "own" });
  assert.equal(own.status, 422);
  assert.deepEqual(await own.json(), { error: { reason: "own_code", message: "This is Desk's own code." } });

  const listed = await request("list");
  assert.deepEqual(await listed.json(), { computers: [{ id: DESK, name: "Desk" }, { id: LAPTOP, name: "Laptop" }] });
});

test("forgetting the browser's computer also clears its choice", async (t) => {
  const gateway = fakeGateway();
  const request = await serve(t, createWebComputers({ gateway, logger: recordingLogger().logger, cookieName: COOKIE }));

  const other = await request("forget", { computer: LAPTOP }, { cookie: `${COOKIE}=${DESK}` });
  assert.equal(other.status, 200);
  assert.equal(other.headers.get("set-cookie"), null);
  const chosen = await request("forget", { computer: DESK }, { cookie: `${COOKIE}=${DESK}` });
  assert.match(chosen.headers.get("set-cookie"), /Max-Age=0/);
  assert.deepEqual(gateway.calls, [["forget", LAPTOP], ["forget", DESK]]);
});

test("requests that cannot be read are refused before the gateway", async (t) => {
  const gateway = fakeGateway();
  const request = await serve(t, createWebComputers({ gateway, logger: recordingLogger().logger, cookieName: COOKIE }));

  assert.equal((await request("use", { computer: "../etc" })).status, 400);
  assert.equal((await request("forget", {})).status, 400);
  assert.equal((await request("pair", { code: 7 })).status, 400);
  assert.equal((await request("pair", { code: "x".repeat(20_000) })).status, 400);
  // A form post, which a browser sends across origins without asking, is not accepted.
  const form = await fetch((await request("list")).url.replace("list", "use"), {
    method: "POST",
    headers: { "content-type": "text/plain" },
    body: JSON.stringify({ computer: DESK }),
  });
  assert.equal(form.status, 400);
  assert.equal((await request("use")).status, 405);
  assert.equal((await request("unknown")).status, 404);
  assert.deepEqual(gateway.calls, [["computers"]]);
});

test("behind an HTTPS proxy the choice is kept only for HTTPS", async (t) => {
  const request = await serve(t, createWebComputers({ gateway: fakeGateway(), logger: recordingLogger().logger, cookieName: COOKIE }));
  const used = await request("use", { computer: DESK }, { "x-forwarded-proto": "https" });
  assert.match(used.headers.get("set-cookie"), /; Secure$/);
});

test("a page opens on the browser's computer while it is still paired", async () => {
  const { events, logger } = recordingLogger();
  const page = (cookie, gateway = fakeGateway()) => createWebComputers({ gateway, logger, cookieName: COOKIE })
    .pageComputer({ headers: cookie === undefined ? {} : { cookie }, socket: {} });

  assert.deepEqual(await page(undefined), { computer: undefined });
  assert.deepEqual(await page(`other=1; ${COOKIE}=${DESK}`), { computer: DESK });
  // Another Web App on this host keeps its own choice.
  assert.deepEqual(await page(`${computerCookieName("/elsewhere/web-app")}=${DESK}`), { computer: undefined });
  const forgotten = await page(`${COOKIE}=${LAPTOP}`);
  assert.equal(forgotten.computer, undefined);
  assert.match(forgotten.setCookie, /Max-Age=0/);
  assert.match((await page(`${COOKIE}=not-an-id`)).setCookie, /Max-Age=0/);
  // When the list cannot be read, the choice stands and that page says why.
  assert.deepEqual(await page(`${COOKIE}=${DESK}`, fakeGateway({ failList: true })), { computer: DESK });
  assert.ok(events.some((entry) => entry.event === "web_computer_choice_unverified" && entry.fields.reason === "gateway_unavailable"));
});

test("a page does not wait on a gateway that never answers", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const { events, logger } = recordingLogger();
  const silent = { ...fakeGateway(), computers: () => new Promise(() => {}) };
  const opening = createWebComputers({ gateway: silent, logger, cookieName: COOKIE, pageChoiceWaitMs: 5_000 })
    .pageComputer({ headers: { cookie: `${COOKIE}=${DESK}` }, socket: {} });
  t.mock.timers.tick(5_000);
  assert.deepEqual(await opening, { computer: DESK });
  assert.ok(events.some((entry) => entry.event === "web_computer_choice_unverified" && entry.fields.reason === "timeout"));
});

test("a page addresses the computer it opened on in its App Server paths", () => {
  assert.equal(appServerBase(undefined), "/__openaide-app-server");
  assert.deepEqual(appServerTarget("/__openaide-app-server/probe"), { computer: undefined, pathname: "/__openaide-app-server/probe" });
  assert.deepEqual(appServerTarget(`/__openaide-app-server/computer/${DESK}/upload/chunk`), {
    computer: DESK,
    pathname: "/__openaide-app-server/upload/chunk",
  });
  assert.equal(appServerTarget("/__openaide-app-server/computer/ABC/probe"), undefined);

  const html = injectBootstrap("<html><body></body></html>", { surface: "task" }, { appServerTransport: "webSocket", computer: DESK });
  assert.match(html, new RegExp(`data-computer-id="${DESK}"`));
  assert.match(html, new RegExp(`&quot;endpointUrl&quot;:&quot;/__openaide-app-server/computer/${DESK}/probe&quot;`));
  const local = injectBootstrap("<html><body></body></html>", { surface: "task" }, {});
  assert.doesNotMatch(local, /data-computer-id/);
  assert.match(local, /&quot;endpointUrl&quot;:&quot;\/__openaide-app-server\/probe&quot;/);
});
