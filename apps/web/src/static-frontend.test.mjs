import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, utimesSync, writeFileSync } from "node:fs";
import http from "node:http";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import zlib from "node:zlib";
import { createStaticFrontend, preferredEncoding } from "./static-frontend.mjs";

const SCRIPT = `globalThis.openaide = ${JSON.stringify("frontend ".repeat(400))};`;

test("unchanged assets revalidate and a rebuilt asset is served again", async (t) => {
  const { origin, root } = await serveFixture(t);

  const first = await fetch(`${origin}/assets/index.js`);
  const etag = first.headers.get("etag");
  assert.equal(first.status, 200);
  assert.equal(first.headers.get("cache-control"), "no-cache");
  assert.equal(await first.text(), SCRIPT);

  const unchanged = await fetch(`${origin}/assets/index.js`, { headers: { "if-none-match": etag } });
  assert.equal(unchanged.status, 304);
  assert.equal(await unchanged.text(), "");

  const rebuilt = `${SCRIPT}\nglobalThis.rebuilt = true;`;
  const scriptPath = path.join(root, "assets", "index.js");
  writeFileSync(scriptPath, rebuilt);
  // timing: data — a distinct modification time, independent of filesystem timestamp resolution.
  utimesSync(scriptPath, new Date(), new Date(Date.now() + 60_000));
  const afterRebuild = await fetch(`${origin}/assets/index.js`, { headers: { "if-none-match": etag } });
  assert.equal(afterRebuild.status, 200);
  assert.notEqual(afterRebuild.headers.get("etag"), etag);
  assert.equal(await afterRebuild.text(), rebuilt);
});

test("text assets are compressed with the encoding the client accepts", async (t) => {
  const { origin } = await serveFixture(t);

  for (const [encoding, decode] of [["br", zlib.brotliDecompressSync], ["gzip", zlib.gunzipSync]]) {
    const response = await rawRequest(`${origin}/assets/index.js`, { "accept-encoding": encoding });
    assert.equal(response.headers["content-encoding"], encoding);
    assert.equal(response.headers.vary, "Accept-Encoding");
    assert.equal(Number(response.headers["content-length"]), response.body.byteLength);
    assert.ok(response.body.byteLength < SCRIPT.length / 2);
    assert.equal(decode(response.body).toString("utf8"), SCRIPT);
  }

  const identity = await rawRequest(`${origin}/assets/index.js`, { "accept-encoding": "br;q=0, gzip;q=0" });
  assert.equal(identity.headers["content-encoding"], undefined);
  assert.equal(identity.body.toString("utf8"), SCRIPT);

  const font = await rawRequest(`${origin}/assets/inter.woff2`, { "accept-encoding": "br, gzip" });
  assert.equal(font.headers["content-type"], "font/woff2");
  assert.equal(font.headers["content-encoding"], undefined);
});

test("product routes receive the bootstrapped document and unknown paths do not", async (t) => {
  const { origin } = await serveFixture(t);

  const task = await fetch(`${origin}/task/task-1`);
  const taskHtml = await task.text();
  assert.equal(task.headers.get("content-type"), "text/html; charset=utf-8");
  assert.match(taskHtml, /data-shell="web"/);
  assert.match(taskHtml, /data-task-id="task-1"/);
  assert.match(taskHtml, /<title>OpenAIDE Fixture<\/title>/);

  // Each route's document carries its own validator, so one route never satisfies another.
  const settings = await fetch(`${origin}/settings`, { headers: { "if-none-match": task.headers.get("etag") } });
  assert.equal(settings.status, 200);
  assert.match(await settings.text(), /data-surface="settings"/);
  const sameTask = await fetch(`${origin}/task/task-1`, { headers: { "if-none-match": task.headers.get("etag") } });
  assert.equal(sameTask.status, 304);

  assert.equal((await fetch(`${origin}/assets/missing.js`)).status, 404);
  assert.equal((await rawRequest(`${origin}/..%2f..%2fetc%2fpasswd`)).status, 404);
  assert.equal((await fetch(`${origin}/assets/index.js`, { method: "POST" })).status, 405);

  const head = await fetch(`${origin}/assets/index.js`, { method: "HEAD" });
  assert.equal(head.status, 200);
  assert.equal(await head.text(), "");
});

test("encoding negotiation prefers brotli and honors refusals", () => {
  assert.equal(preferredEncoding("gzip, deflate, br"), "br");
  assert.equal(preferredEncoding("gzip, br;q=0"), "gzip");
  assert.equal(preferredEncoding("identity"), undefined);
  assert.equal(preferredEncoding(undefined), undefined);
});

async function serveFixture(t) {
  const root = mkdtempSync(path.join(tmpdir(), "openaide-web-static-"));
  mkdirSync(path.join(root, "assets"));
  writeFileSync(path.join(root, "index.html"), "<html><head><title>x</title></head><body><div id=\"root\"></div></body></html>");
  writeFileSync(path.join(root, "assets", "index.js"), SCRIPT);
  writeFileSync(path.join(root, "assets", "inter.woff2"), Buffer.alloc(4096, 7));
  const frontend = createStaticFrontend({
    root,
    presentation: { appServerTransport: "webSocket", title: "OpenAIDE Fixture" },
    logger: { warn() {} },
  });
  const server = http.createServer((req, res) => {
    void frontend.request(req, res, new URL(req.url, "http://127.0.0.1"));
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  t.after(() => {
    server.closeAllConnections();
    server.close();
    rmSync(root, { recursive: true, force: true });
  });
  return { origin: `http://127.0.0.1:${server.address().port}`, root };
}

// `fetch` decodes bodies and normalizes paths; this reads what was actually sent.
function rawRequest(url, headers = {}) {
  const target = new URL(url);
  return new Promise((resolve, reject) => {
    const request = http.request({
      hostname: target.hostname,
      port: target.port,
      path: url.slice(target.origin.length),
      headers,
    }, (response) => {
      const chunks = [];
      response.on("data", (chunk) => chunks.push(chunk));
      response.on("end", () => resolve({
        body: Buffer.concat(chunks),
        headers: response.headers,
        status: response.statusCode,
      }));
    });
    request.once("error", reject);
    request.end();
  });
}
